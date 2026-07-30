'use client';

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ChainDisplayId } from '@/lib/cross-chain/types';
import { StellarRouteApiError } from '@/lib/api/client';
import { buildCctpQuoteRequest } from '@/lib/cctp/corridor-bridge';
import { getCctpApiClient } from '@/lib/cctp/client';
import { mapCctpError, type CctpTraderError } from '@/lib/cctp/errors';
import { fingerprintPreparedPayload } from '@/lib/cctp/payload-fingerprint';
import {
  buildCctpSessionRecord,
  clearCctpSession,
  clearPendingEvmTx,
  loadCctpSession,
  patchCctpSessionRecovery,
  purgeCctpSessionIfTerminal,
  saveCctpSession,
  sessionRecoveryMatchesInputs,
  setPendingEvmTx,
  type BurnPrepareStep,
  type CctpSessionRecord,
} from '@/lib/cctp/session-vault';
import {
  executePreparedPayload,
  reconcileEvmTransactionHash,
} from '@/lib/cctp/wallet-execution';
import { startCctpStatusPoll, type StatusPollHandle } from '@/lib/cctp/status-poll';
import type {
  CctpPrepareBurnResponse,
  CctpQuoteResponse,
  CctpTransferStatus,
  CctpTransferStatusResponse,
} from '@/lib/cctp/types';

export type CctpSagaStage =
  | 'idle'
  | 'quoting'
  | 'quoted'
  | 'sign_approval'
  | 'sign_burn'
  | 'sign_mint'
  | 'awaiting_attestation'
  | 'completed'
  | 'failed'
  | 'unavailable'
  | 'resume_pending'
  | 'pending_reconcile';

export interface CctpWalletRoles {
  sourceStellarAdapterId?: string;
  sourceEvmAdapterId?: string;
  evmDestinationAdapterId?: string;
  mintSubmitterStellarAdapterId?: string;
  sourceAddress?: string;
  recipient: string;
  mintSubmitter?: string;
}

export interface UseCctpSagaInput {
  sourceChainId: ChainDisplayId;
  destChainId: ChainDisplayId;
  amount: string;
  recipient: string;
  sender?: string;
  mintSubmitter?: string;
  wallets: CctpWalletRoles;
  bridgeReady: boolean;
  quoteInputsKey: string;
}

const TERMINAL_STAGES = new Set<CctpSagaStage>([
  'idle',
  'completed',
  'failed',
  'unavailable',
]);

export function useCctpSaga(input: UseCctpSagaInput) {
  const client = useMemo(() => getCctpApiClient(), []);
  const [stage, setStage] = useState<CctpSagaStage>('idle');
  const [quote, setQuote] = useState<CctpQuoteResponse | null>(null);
  const [transferStatus, setTransferStatus] =
    useState<CctpTransferStatusResponse | null>(null);
  const [error, setError] = useState<CctpTraderError | null>(null);
  const [session, setSession] = useState<CctpSessionRecord | null>(null);
  const [idempotencyKey, setIdempotencyKey] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [burnPrepareStep, setBurnPrepareStep] = useState<BurnPrepareStep>('unknown');
  const [resumeMismatch, setResumeMismatch] = useState(false);
  const [reattestCooldownUntil, setReattestCooldownUntil] = useState<number | null>(
    null,
  );
  const pollRef = useRef<StatusPollHandle | null>(null);
  const lastInputsKey = useRef<string | null>(null);
  const walletRequestCount = useRef(0);
  const prepareBurnCallCount = useRef(0);
  const lastPreparedFingerprint = useRef<string | null>(null);
  const lastPrepared = useRef<CctpPrepareBurnResponse | null>(null);

  const stopPoll = useCallback(() => {
    pollRef.current?.stop();
    pollRef.current = null;
  }, []);

  useEffect(() => () => stopPoll(), [stopPoll]);

  const syncSession = useCallback((record: CctpSessionRecord | null) => {
    setSession(record);
    if (record?.recovery.burnPrepareStep) {
      setBurnPrepareStep(record.recovery.burnPrepareStep);
    }
    if (record?.recovery.pendingEvmTx) {
      setStage('pending_reconcile');
    }
    if (record?.recovery.lastPreparedFingerprint) {
      lastPreparedFingerprint.current = record.recovery.lastPreparedFingerprint;
    }
  }, []);

  const inputsLocked = useMemo(
    () =>
      Boolean(session) &&
      !['idle', 'completed', 'failed', 'unavailable'].includes(stage),
    [session, stage],
  );

  useEffect(() => {
    if (
      lastInputsKey.current !== null &&
      lastInputsKey.current !== input.quoteInputsKey &&
      !inputsLocked
    ) {
      setIdempotencyKey(crypto.randomUUID());
      setQuote(null);
      setBurnPrepareStep('unknown');
      if (stage === 'quoted') setStage('idle');
    }
    lastInputsKey.current = input.quoteInputsKey;
  }, [input.quoteInputsKey, inputsLocked, stage]);

  useEffect(() => {
    const loaded = loadCctpSession();
    if (!loaded.ok) return;
    if (!sessionRecoveryMatchesInputs(loaded.record, input)) {
      syncSession(loaded.record);
      setResumeMismatch(true);
      setStage('resume_pending');
      return;
    }
    syncSession(loaded.record);
    setIdempotencyKey(loaded.record.idempotencyKey);
    setStage(loaded.record.recovery.pendingEvmTx ? 'pending_reconcile' : 'resume_pending');
    // eslint-disable-next-line react-hooks/exhaustive-deps -- mount-only session restore
  }, []);

  const accessOptions = useCallback(() => {
    if (!session) return undefined;
    return { accessToken: session.accessToken };
  }, [session]);

  const applyReattestCooldown = useCallback(
    (status: CctpTransferStatusResponse) => {
      if (status.reattest_cooldown_until) {
        setReattestCooldownUntil(status.reattest_cooldown_until * 1000);
      }
    },
    [],
  );

  const handleStatus = useCallback(
    (status: CctpTransferStatusResponse) => {
      setTransferStatus(status);
      applyReattestCooldown(status);
      mapStageFromStatus(status.status, setStage);
      purgeCctpSessionIfTerminal(status.status);
      if (status.status === 'completed') {
        stopPoll();
        clearCctpSession();
        setSession(null);
        setBurnPrepareStep('unknown');
      }
    },
    [applyReattestCooldown, stopPoll],
  );

  const startPoll = useCallback(
    (transferId: string, token: string) => {
      stopPoll();
      pollRef.current = startCctpStatusPoll({
        client,
        transferId,
        accessToken: token,
        callbacks: {
          onUpdate: handleStatus,
          onError: (err) => setError(mapCctpError(err)),
        },
      });
    },
    [client, handleStatus, stopPoll],
  );

  const persistBurnPrepare = useCallback(
    (prepared: CctpPrepareBurnResponse) => {
      const step: BurnPrepareStep = prepared.approval_required
        ? 'approval_ready'
        : 'burn_ready';
      const fingerprint = fingerprintPreparedPayload(prepared.payload);
      lastPreparedFingerprint.current = fingerprint;
      lastPrepared.current = prepared;
      setBurnPrepareStep(step);
      const patched = patchCctpSessionRecovery({
        burnPrepareStep: step,
        lastPreparedFingerprint: fingerprint,
      });
      if (patched) setSession(patched);
      return step;
    },
    [],
  );

  const requestQuote = useCallback(async () => {
    if (!input.bridgeReady) {
      setStage('unavailable');
      setError(
        mapCctpError(
          new StellarRouteApiError(503, 'cctp_not_enabled', 'CCTP not enabled'),
        ),
      );
      return;
    }
    setBusy(true);
    setError(null);
    setStage('quoting');
    setBurnPrepareStep('unknown');
    const key = idempotencyKey ?? crypto.randomUUID();
    setIdempotencyKey(key);

    const body = buildCctpQuoteRequest({
      sourceChainId: input.sourceChainId,
      destChainId: input.destChainId,
      amount: input.amount,
      recipient: input.recipient,
      sender: input.sender,
      mintSubmitter: input.mintSubmitter,
    });
    if (!body) {
      setError({
        kind: 'nonretryable',
        title: 'Unsupported corridor',
        message: 'This chain pair is not a CCTP corridor.',
      });
      setStage('failed');
      setBusy(false);
      return;
    }

    try {
      const response = await client.quote(body, { idempotencyKey: key });
      setQuote(response);
      const record = buildCctpSessionRecord({
        transferId: response.transfer_id,
        accessToken: response.access_token,
        idempotencyKey: key,
        quoteExpiresAt: response.expires_at,
        recovery: {
          corridorId: response.corridor_id,
          direction: response.direction,
          sourceChainId: input.sourceChainId,
          destChainId: input.destChainId,
          amount: input.amount,
          recipient: input.recipient,
          quoteExpiresAt: response.expires_at,
          burnPrepareStep: 'unknown',
        },
      });
      saveCctpSession(record);
      syncSession(record);
      setResumeMismatch(false);
      setStage('quoted');
    } catch (err) {
      setError(mapCctpError(err));
      setStage('failed');
    } finally {
      setBusy(false);
    }
  }, [client, idempotencyKey, input, syncSession]);

  const prepareSourceBurn = useCallback(async () => {
    if (!session) return;
    setBusy(true);
    setError(null);
    try {
      prepareBurnCallCount.current += 1;
      const prepared = await client.prepareBurn(session.transferId, accessOptions());
      persistBurnPrepare(prepared);
      setStage('quoted');
    } catch (err) {
      setError(mapCctpError(err));
    } finally {
      setBusy(false);
    }
  }, [accessOptions, client, persistBurnPrepare, session]);

  const reconcileOnLoad = useCallback(async () => {
    const loaded = loadCctpSession();
    if (!loaded.ok) {
      if (loaded.reason === 'expired' || loaded.reason === 'invalid') {
        setError({
          kind: 'authorization_lost',
          title: 'Session expired',
          message: 'Start a new quote to continue.',
        });
      }
      return;
    }
    syncSession(loaded.record);
    if (!sessionRecoveryMatchesInputs(loaded.record, input)) {
      setResumeMismatch(true);
      setStage('resume_pending');
      return;
    }
    try {
      const status = await client.getTransfer(loaded.record.transferId, {
        accessToken: loaded.record.accessToken,
      });
      handleStatus(status);
      if (loaded.record.recovery.pendingEvmTx) {
        setStage('pending_reconcile');
      } else if (loaded.record.recovery.burnPrepareStep) {
        setBurnPrepareStep(loaded.record.recovery.burnPrepareStep);
        setStage('quoted');
      }
      if (!['completed', 'cancelled', 'provider_killed'].includes(status.status)) {
        startPoll(loaded.record.transferId, loaded.record.accessToken);
      }
      setResumeMismatch(false);
    } catch {
      setError({
        kind: 'authorization_lost',
        title: 'Cannot resume transfer',
        message:
          'Start a new quote — the prior access token is no longer valid.',
      });
      clearCctpSession();
      setSession(null);
      setStage('idle');
    }
  }, [client, handleStatus, input, startPoll, syncSession]);

  const resolveEvmAdapterForPayload = useCallback(
    (payloadType: string) => {
      if (payloadType === 'evm_transaction') {
        return (
          input.wallets.sourceEvmAdapterId ??
          input.wallets.evmDestinationAdapterId
        );
      }
      return undefined;
    },
    [input.wallets],
  );

  const signApprovalStep = useCallback(async () => {
    if (!session) {
      setError({
        kind: 'authorization_lost',
        title: 'No active transfer',
        message: 'Request a quote first.',
      });
      return;
    }
    const prepared = lastPrepared.current;
    if (!prepared?.approval_required || burnPrepareStep !== 'approval_ready') {
      setError({
        kind: 'nonretryable',
        title: 'Prepare approval first',
        message: 'Prepare the source transaction before approving USDC spend.',
      });
      return;
    }
    setBusy(true);
    setError(null);
    try {
      setStage('sign_approval');
      walletRequestCount.current += 1;
      const exec = await executePreparedPayload({
        payload: prepared.payload,
        stellarAdapterId: input.wallets.sourceStellarAdapterId,
        evmAdapterId: resolveEvmAdapterForPayload(prepared.payload.type),
        expiresAtSec: prepared.expires_at,
        walletNetwork: 'testnet',
      });
      if (!exec.submissionReady) {
        const patched = setPendingEvmTx({
          txHash: exec.txHash,
          purpose: 'approval',
        });
        if (patched) syncSession(patched);
        setStage('pending_reconcile');
        return;
      }
      await client.submitBurn(session.transferId, { tx_hash: exec.txHash }, accessOptions());
      clearPendingEvmTx();
      const status = await client.getTransfer(session.transferId, accessOptions());
      handleStatus(status);
      lastPrepared.current = null;
      setBurnPrepareStep('unknown');
      patchCctpSessionRecovery({ burnPrepareStep: 'unknown' });
      setStage('quoted');
    } catch (err) {
      setError(mapCctpError(err));
      setStage('failed');
    } finally {
      setBusy(false);
    }
  }, [
    accessOptions,
    burnPrepareStep,
    client,
    handleStatus,
    input.wallets,
    resolveEvmAdapterForPayload,
    session,
    syncSession,
  ]);

  const signBurnStep = useCallback(async () => {
    if (!session) {
      setError({
        kind: 'authorization_lost',
        title: 'No active transfer',
        message: 'Request a quote first.',
      });
      return;
    }
    const prepared = lastPrepared.current;
    if (!prepared || prepared.approval_required || burnPrepareStep !== 'burn_ready') {
      setError({
        kind: 'nonretryable',
        title: 'Prepare burn first',
        message: 'Prepare the source transaction before signing the burn.',
      });
      return;
    }
    setBusy(true);
    setError(null);
    try {
      setStage('sign_burn');
      walletRequestCount.current += 1;
      const exec = await executePreparedPayload({
        payload: prepared.payload,
        stellarAdapterId: input.wallets.sourceStellarAdapterId,
        evmAdapterId: resolveEvmAdapterForPayload(prepared.payload.type),
        expiresAtSec: prepared.expires_at,
        walletNetwork: 'testnet',
      });
      if (!exec.submissionReady) {
        const patched = setPendingEvmTx({ txHash: exec.txHash, purpose: 'burn' });
        if (patched) syncSession(patched);
        setStage('pending_reconcile');
        return;
      }
      await client.submitBurn(session.transferId, { tx_hash: exec.txHash }, accessOptions());
      clearPendingEvmTx();
      lastPrepared.current = null;
      setBurnPrepareStep('unknown');
      patchCctpSessionRecovery({ burnPrepareStep: 'unknown' });
      setStage('awaiting_attestation');
      startPoll(session.transferId, session.accessToken);
    } catch (err) {
      setError(mapCctpError(err));
      setStage('failed');
    } finally {
      setBusy(false);
    }
  }, [
    accessOptions,
    burnPrepareStep,
    client,
    input.wallets,
    resolveEvmAdapterForPayload,
    session,
    startPoll,
    syncSession,
  ]);

  const signPreparedMintStep = useCallback(async () => {
    if (!session) return;
    setBusy(true);
    setError(null);
    try {
      const prepared = await client.prepareMint(session.transferId, accessOptions());
      setStage('sign_mint');
      walletRequestCount.current += 1;
      const stellarMintId =
        input.wallets.mintSubmitterStellarAdapterId ??
        input.wallets.sourceStellarAdapterId;
      const evmMintId = input.wallets.evmDestinationAdapterId;
      const exec = await executePreparedPayload({
        payload: prepared.payload,
        stellarAdapterId:
          prepared.payload.type === 'stellar_xdr' ? stellarMintId : undefined,
        evmAdapterId:
          prepared.payload.type === 'evm_transaction' ? evmMintId : undefined,
        expiresAtSec: prepared.expires_at,
        walletNetwork: 'testnet',
      });
      if (!exec.submissionReady) {
        const patched = setPendingEvmTx({ txHash: exec.txHash, purpose: 'mint' });
        if (patched) syncSession(patched);
        setStage('pending_reconcile');
        return;
      }
      await client.submitMint(session.transferId, { tx_hash: exec.txHash }, accessOptions());
      clearPendingEvmTx();
      startPoll(session.transferId, session.accessToken);
    } catch (err) {
      setError(mapCctpError(err));
      setStage('failed');
    } finally {
      setBusy(false);
    }
  }, [accessOptions, client, input.wallets, session, startPoll, syncSession]);

  const reconcilePendingEvmTx = useCallback(async () => {
    const loaded = loadCctpSession();
    const pending = loaded.ok ? loaded.record.recovery.pendingEvmTx : session?.recovery.pendingEvmTx;
    if (!session || !pending) return;
    setBusy(true);
    setError(null);
    try {
      const exec = await reconcileEvmTransactionHash({ txHash: pending.txHash });
      if (!exec.submissionReady) {
        setStage('pending_reconcile');
        return;
      }
      if (pending.purpose === 'mint') {
        await client.submitMint(session.transferId, { tx_hash: exec.txHash }, accessOptions());
      } else {
        await client.submitBurn(session.transferId, { tx_hash: exec.txHash }, accessOptions());
      }
      clearPendingEvmTx();
      if (pending.purpose === 'approval') {
        const status = await client.getTransfer(session.transferId, accessOptions());
        handleStatus(status);
        setBurnPrepareStep('unknown');
        patchCctpSessionRecovery({ burnPrepareStep: 'unknown' });
        setStage('quoted');
      } else if (pending.purpose === 'burn') {
        setStage('awaiting_attestation');
        startPoll(session.transferId, session.accessToken);
      } else {
        startPoll(session.transferId, session.accessToken);
      }
    } catch (err) {
      setError(mapCctpError(err));
    } finally {
      setBusy(false);
    }
  }, [
    accessOptions,
    client,
    handleStatus,
    session,
    startPoll,
  ]);

  const reattest = useCallback(async () => {
    if (!session) return;
    if (reattestCooldownUntil && Date.now() < reattestCooldownUntil) return;
    setBusy(true);
    setError(null);
    try {
      const result = await client.reattest(session.transferId, accessOptions());
      handleStatus({
        transfer_id: result.transfer_id,
        corridor_id: quote?.corridor_id ?? session.recovery.corridorId,
        provider: quote?.provider ?? 'circle-cctp',
        direction: session.recovery.direction,
        status: result.status,
        retryable: result.retryable,
      });
      startPoll(session.transferId, session.accessToken);
    } catch (err) {
      if (err instanceof StellarRouteApiError && err.status === 409) {
        try {
          const status = await client.getTransfer(session.transferId, accessOptions());
          handleStatus(status);
        } catch {
          // fall through to mapped error
        }
      }
      setError(mapCctpError(err));
    } finally {
      setBusy(false);
    }
  }, [
    accessOptions,
    client,
    handleStatus,
    quote,
    reattestCooldownUntil,
    session,
    startPoll,
  ]);

  const resetSaga = useCallback(() => {
    stopPoll();
    clearCctpSession();
    setSession(null);
    setQuote(null);
    setTransferStatus(null);
    setError(null);
    setStage('idle');
    setIdempotencyKey(null);
    setBurnPrepareStep('unknown');
    setResumeMismatch(false);
    setReattestCooldownUntil(null);
    walletRequestCount.current = 0;
    prepareBurnCallCount.current = 0;
    lastPreparedFingerprint.current = null;
    lastPrepared.current = null;
  }, [stopPoll]);

  const pendingEvmTx = session?.recovery.pendingEvmTx ?? null;

  const primaryAction = useMemo(() => {
    if (!input.bridgeReady) {
      return { label: 'Bridge unavailable', disabled: true, action: 'none' as const };
    }
    if (stage === 'pending_reconcile' || pendingEvmTx) {
      return {
        label: 'Transaction pending — reconcile',
        disabled: busy,
        action: 'reconcile_pending' as const,
      };
    }
    if (stage === 'resume_pending') {
      return {
        label: 'Resume transfer',
        disabled: busy,
        action: 'resume' as const,
      };
    }
    if (stage === 'idle' || stage === 'failed') {
      return { label: 'Get CCTP quote', disabled: busy, action: 'quote' as const };
    }
    if (stage === 'quoted') {
      if (burnPrepareStep === 'unknown') {
        return {
          label: 'Prepare source transaction',
          disabled: busy,
          action: 'prepare' as const,
        };
      }
      if (burnPrepareStep === 'approval_ready') {
        return {
          label: 'Approve USDC spend',
          disabled: busy,
          action: 'approve' as const,
        };
      }
      return {
        label: 'Sign burn on source chain',
        disabled: busy,
        action: 'burn' as const,
      };
    }
    if (
      transferStatus?.status === 'attestation_ready' ||
      transferStatus?.status === 'mint_prepared' ||
      transferStatus?.status === 'mint_failed_retryable'
    ) {
      return {
        label: 'Sign mint on destination',
        disabled: busy,
        action: 'mint' as const,
      };
    }
    if (transferStatus?.status === 'attestation_failed') {
      const cooldownActive =
        reattestCooldownUntil !== null && Date.now() < reattestCooldownUntil;
      return {
        label: cooldownActive ? 'Retry attestation (cooldown)' : 'Retry attestation',
        disabled: busy || cooldownActive,
        action: 'reattest' as const,
      };
    }
    return { label: 'Waiting…', disabled: true, action: 'none' as const };
  }, [
    busy,
    burnPrepareStep,
    input.bridgeReady,
    pendingEvmTx,
    reattestCooldownUntil,
    stage,
    transferStatus?.status,
  ]);

  const runPrimaryAction = useCallback(async () => {
    switch (primaryAction.action) {
      case 'quote':
        await requestQuote();
        break;
      case 'prepare':
        await prepareSourceBurn();
        break;
      case 'approve':
        await signApprovalStep();
        break;
      case 'burn':
        await signBurnStep();
        break;
      case 'mint':
        await signPreparedMintStep();
        break;
      case 'reattest':
        await reattest();
        break;
      case 'reconcile_pending':
        await reconcilePendingEvmTx();
        break;
      case 'resume':
        await reconcileOnLoad();
        break;
      default:
        break;
    }
  }, [
    primaryAction.action,
    prepareSourceBurn,
    reattest,
    reconcileOnLoad,
    reconcilePendingEvmTx,
    requestQuote,
    signApprovalStep,
    signBurnStep,
    signPreparedMintStep,
  ]);

  return {
    stage,
    quote,
    transferStatus,
    error,
    busy,
    inputsLocked,
    burnPrepareStep,
    resumeMismatch,
    pendingEvmTx,
    sessionPublic: session
      ? { transferId: session.transferId, recovery: session.recovery }
      : null,
    primaryAction,
    runPrimaryAction,
    requestQuote,
    prepareSourceBurn,
    reconcileOnLoad,
    reconcilePendingEvmTx,
    resetSaga,
    reattestCooldownUntil,
    getWalletRequestCount: () => walletRequestCount.current,
    getPrepareBurnCallCount: () => prepareBurnCallCount.current,
    getLastPreparedFingerprint: () => lastPreparedFingerprint.current,
    signApprovalStep,
    signBurnStep,
    signPreparedMintStep,
  };
}

function mapStageFromStatus(
  status: CctpTransferStatus,
  setStage: (s: CctpSagaStage) => void,
) {
  switch (status) {
    case 'completed':
      setStage('completed');
      break;
    case 'awaiting_attestation':
    case 'burn_submitted':
      setStage('awaiting_attestation');
      break;
    case 'attestation_ready':
    case 'mint_prepared':
    case 'mint_failed_retryable':
      setStage('sign_mint');
      break;
    case 'attestation_failed':
      setStage('failed');
      break;
    case 'provider_killed':
      setStage('unavailable');
      break;
    default:
      break;
  }
}

export function isCctpSagaTerminal(stage: CctpSagaStage): boolean {
  return TERMINAL_STAGES.has(stage);
}
