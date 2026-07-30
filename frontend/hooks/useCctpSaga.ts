'use client';

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ChainDisplayId } from '@/lib/cross-chain/types';
import { StellarRouteApiError } from '@/lib/api/client';
import { StrKey } from '@stellar/stellar-base';
import { buildCctpQuoteRequest } from '@/lib/cctp/corridor-bridge';
import { getCctpApiClient } from '@/lib/cctp/client';
import { mapCctpError, type CctpTraderError } from '@/lib/cctp/errors';
import {
  buildCctpSessionRecord,
  clearCctpSession,
  loadCctpSession,
  purgeCctpSessionIfTerminal,
  saveCctpSession,
  type CctpSessionRecord,
} from '@/lib/cctp/session-vault';
import { executePreparedPayload } from '@/lib/cctp/wallet-execution';
import { startCctpStatusPoll, type StatusPollHandle } from '@/lib/cctp/status-poll';
import type {
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
  | 'unavailable';

export interface CctpWalletRoles {
  sourceStellarAdapterId?: string;
  sourceEvmAdapterId?: string;
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
  const pollRef = useRef<StatusPollHandle | null>(null);
  const lastInputsKey = useRef<string | null>(null);

  const stopPoll = useCallback(() => {
    pollRef.current?.stop();
    pollRef.current = null;
  }, []);

  useEffect(() => () => stopPoll(), [stopPoll]);

  useEffect(() => {
    if (lastInputsKey.current !== null && lastInputsKey.current !== input.quoteInputsKey) {
      setIdempotencyKey(crypto.randomUUID());
      setQuote(null);
      if (stage === 'quoted') setStage('idle');
    }
    lastInputsKey.current = input.quoteInputsKey;
  }, [input.quoteInputsKey, stage]);

  useEffect(() => {
    const loaded = loadCctpSession();
    if (!loaded.ok) return;
    if (loaded.record.recovery.corridorId) {
      setSession(loaded.record);
      setIdempotencyKey(loaded.record.idempotencyKey);
      setStage('quoted');
    }
  }, []);

  const accessOptions = useCallback(() => {
    if (!session) return undefined;
    return { accessToken: session.accessToken };
  }, [session]);

  const handleStatus = useCallback(
    (status: CctpTransferStatusResponse) => {
      setTransferStatus(status);
      mapStageFromStatus(status.status, setStage);
      purgeCctpSessionIfTerminal(status.status);
      if (status.status === 'completed') {
        stopPoll();
        clearCctpSession();
        setSession(null);
      }
    },
    [stopPoll],
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
        },
      });
      saveCctpSession(record);
      setSession(record);
      setStage('quoted');
    } catch (err) {
      setError(mapCctpError(err));
      setStage('failed');
    } finally {
      setBusy(false);
    }
  }, [client, idempotencyKey, input]);

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
    setSession(loaded.record);
    try {
      const status = await client.getTransfer(loaded.record.transferId, {
        accessToken: loaded.record.accessToken,
      });
      handleStatus(status);
      if (!['completed', 'cancelled', 'provider_killed'].includes(status.status)) {
        startPoll(loaded.record.transferId, loaded.record.accessToken);
      }
    } catch {
      setError({
        kind: 'authorization_lost',
        title: 'Cannot resume transfer',
        message: 'Start a new quote — the prior access token is no longer valid.',
      });
      clearCctpSession();
      setSession(null);
      setStage('idle');
    }
  }, [client, handleStatus, startPoll]);

  const signPreparedBurnStep = useCallback(async () => {
    if (!session) {
      setError({
        kind: 'authorization_lost',
        title: 'No active transfer',
        message: 'Request a quote first.',
      });
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const prepared = await client.prepareBurn(session.transferId, accessOptions());
      const isApproval = prepared.approval_required === true;
      setStage(isApproval ? 'sign_approval' : 'sign_burn');
      const { txHash } = await executePreparedPayload({
        payload: prepared.payload,
        stellarAdapterId: input.wallets.sourceStellarAdapterId,
        evmAdapterId: input.wallets.sourceEvmAdapterId,
        expiresAtSec: prepared.expires_at,
        walletNetwork: 'testnet',
      });
      await client.submitBurn(session.transferId, { tx_hash: txHash }, accessOptions());
      if (isApproval) {
        const burnPrep = await client.prepareBurn(session.transferId, accessOptions());
        setStage('sign_burn');
        const burnExec = await executePreparedPayload({
          payload: burnPrep.payload,
          stellarAdapterId: input.wallets.sourceStellarAdapterId,
          evmAdapterId: input.wallets.sourceEvmAdapterId,
          expiresAtSec: burnPrep.expires_at,
          walletNetwork: 'testnet',
        });
        await client.submitBurn(
          session.transferId,
          { tx_hash: burnExec.txHash },
          accessOptions(),
        );
      }
      setStage('awaiting_attestation');
      startPoll(session.transferId, session.accessToken);
    } catch (err) {
      setError(mapCctpError(err));
      setStage('failed');
    } finally {
      setBusy(false);
    }
  }, [accessOptions, client, input.wallets, session, startPoll]);

  const signPreparedMintStep = useCallback(async () => {
    if (!session) return;
    setBusy(true);
    setError(null);
    try {
      const prepared = await client.prepareMint(session.transferId, accessOptions());
      setStage('sign_mint');
      const { txHash } = await executePreparedPayload({
        payload: prepared.payload,
        stellarAdapterId:
          input.wallets.mintSubmitterStellarAdapterId ??
          input.wallets.sourceStellarAdapterId,
        evmAdapterId: input.wallets.sourceEvmAdapterId,
        expiresAtSec: prepared.expires_at,
        walletNetwork: 'testnet',
      });
      await client.submitMint(session.transferId, { tx_hash: txHash }, accessOptions());
      startPoll(session.transferId, session.accessToken);
    } catch (err) {
      setError(mapCctpError(err));
      setStage('failed');
    } finally {
      setBusy(false);
    }
  }, [accessOptions, client, input.wallets, session, startPoll]);

  const reattest = useCallback(async () => {
    if (!session) return;
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
      setError(mapCctpError(err));
    } finally {
      setBusy(false);
    }
  }, [accessOptions, client, handleStatus, quote, session, startPoll]);

  const resetSaga = useCallback(() => {
    stopPoll();
    clearCctpSession();
    setSession(null);
    setQuote(null);
    setTransferStatus(null);
    setError(null);
    setStage('idle');
    setIdempotencyKey(null);
  }, [stopPoll]);

  const primaryAction = useMemo(() => {
    if (!input.bridgeReady) {
      return { label: 'Bridge unavailable', disabled: true, action: 'none' as const };
    }
    if (stage === 'idle' || stage === 'failed') {
      return { label: 'Get CCTP quote', disabled: busy, action: 'quote' as const };
    }
    if (stage === 'quoted') {
      return { label: 'Sign burn on source chain', disabled: busy, action: 'burn' as const };
    }
    if (
      transferStatus?.status === 'attestation_ready' ||
      transferStatus?.status === 'mint_prepared' ||
      transferStatus?.status === 'mint_failed_retryable'
    ) {
      return { label: 'Sign mint on destination', disabled: busy, action: 'mint' as const };
    }
    if (transferStatus?.status === 'attestation_failed') {
      return { label: 'Retry attestation', disabled: busy, action: 'reattest' as const };
    }
    return { label: 'Waiting…', disabled: true, action: 'none' as const };
  }, [busy, input.bridgeReady, stage, transferStatus?.status]);

  const runPrimaryAction = useCallback(async () => {
    switch (primaryAction.action) {
      case 'quote':
        await requestQuote();
        break;
      case 'burn':
        await signPreparedBurnStep();
        break;
      case 'mint':
        await signPreparedMintStep();
        break;
      case 'reattest':
        await reattest();
        break;
      default:
        break;
    }
  }, [
    primaryAction.action,
    reattest,
    requestQuote,
    signPreparedBurnStep,
    signPreparedMintStep,
  ]);

  return {
    stage,
    quote,
    transferStatus,
    error,
    busy,
    sessionPublic: session
      ? { transferId: session.transferId, recovery: session.recovery }
      : null,
    primaryAction,
    runPrimaryAction,
    requestQuote,
    reconcileOnLoad,
    resetSaga,
    resolveMuxedMintSubmitter: (recipient: string, connectedG?: string) => {
      if (StrKey.isValidMed25519PublicKey(recipient.trim())) {
        return connectedG;
      }
      return connectedG;
    },
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
