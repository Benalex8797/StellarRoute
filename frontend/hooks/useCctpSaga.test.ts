import { renderHook, act } from '@testing-library/react';
import { StrictMode, createElement, type ReactNode } from 'react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { useCctpSaga } from './useCctpSaga';
import { fingerprintPreparedPayload } from '@/lib/cctp/payload-fingerprint';
import { buildCctpSessionRecord } from '@/lib/cctp/session-vault';

const prepareBurn = vi.fn();
const submitBurn = vi.fn();
const getTransfer = vi.fn();
const executePreparedPayload = vi.fn();
const startPoll = vi.fn(() => ({ stop: vi.fn() }));

vi.mock('@/lib/cctp/client', () => ({
  getCctpApiClient: () => ({
    quote: vi.fn().mockResolvedValue({
      transfer_id: 't1',
      access_token: 'tok',
      corridor_id: 'c',
      provider: 'circle-cctp',
      direction: 'evm_to_stellar',
      source_amount: '10',
      destination_amount: '9.9',
      fee_quote: {},
      expires_at: 9999999999,
      finality: 'standard',
    }),
    prepareBurn,
    submitBurn,
    getTransfer,
  }),
}));

vi.mock('@/lib/cctp/wallet-execution', () => ({
  executePreparedPayload: (...args: unknown[]) => executePreparedPayload(...args),
  reconcileEvmTransactionHash: vi.fn(),
}));

vi.mock('@/lib/cctp/status-poll', () => ({
  startCctpStatusPoll: (...args: unknown[]) => startPoll(...args),
}));

const evmApprovalPayload = {
  type: 'evm_transaction' as const,
  chain_id: 'eip155:11155111',
  to: '0x1c7d4b196cb0c7b01d743fbc6116a902379c7238',
  data: '0xapprove',
  value: '0',
};

const evmBurnPayload = {
  ...evmApprovalPayload,
  data: '0xburn',
};

const stellarApprovalPayload = {
  type: 'stellar_xdr' as const,
  network_passphrase: 'Test SDF Network ; September 2015',
  xdr_envelope: 'AAAAapproval',
};

const stellarBurnPayload = {
  ...stellarApprovalPayload,
  xdr_envelope: 'AAAAburn',
};

const baseInput = {
  sourceChainId: 'ethereum-sepolia' as const,
  destChainId: 'stellar' as const,
  amount: '10',
  recipient: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
  wallets: {
    recipient: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
    sourceEvmAdapterId: 'evm:test',
  },
  bridgeReady: true,
  quoteInputsKey: 'k1',
};

function seedSession(overrides?: {
  burnPrepareStep?: 'approval_ready' | 'burn_ready' | 'reprepare_required';
  fingerprint?: string;
}) {
  const fp = overrides?.fingerprint ?? 'vault-fp';
  const record = buildCctpSessionRecord({
    transferId: 't1',
    accessToken: 'tok',
    idempotencyKey: 'idem-1',
    recovery: {
      corridorId: 'c',
      direction: 'evm_to_stellar',
      sourceChainId: 'ethereum-sepolia',
      destChainId: 'stellar',
      amount: '10',
      recipient: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
      burnPrepareStep: overrides?.burnPrepareStep ?? 'approval_ready',
      lastPreparedFingerprint: fp,
    },
  });
  sessionStorage.setItem('stellarroute:cctp:v1', JSON.stringify(record));
  return record;
}

describe('useCctpSaga server-driven burn staging', () => {
  beforeEach(() => {
    sessionStorage.clear();
    prepareBurn.mockReset();
    submitBurn.mockReset();
    getTransfer.mockReset();
    executePreparedPayload.mockReset();
    startPoll.mockClear();
    executePreparedPayload.mockResolvedValue({ txHash: '0xhash', submissionReady: true });
    submitBurn.mockResolvedValue({ status: 'burn_submitted' });
    getTransfer.mockResolvedValue({
      transfer_id: 't1',
      corridor_id: 'c',
      provider: 'circle-cctp',
      direction: 'evm_to_stellar',
      status: 'burn_prepared',
      retryable: false,
    });
  });

  it('EVM: prepare → approve (1 wallet) → prepare → burn (1 wallet)', async () => {
    const input = {
      sourceChainId: 'ethereum-sepolia' as const,
      destChainId: 'stellar' as const,
      amount: '10',
      recipient: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
      wallets: {
        recipient: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
        sourceEvmAdapterId: 'evm:test',
      },
      bridgeReady: true,
      quoteInputsKey: 'k1',
    };
    prepareBurn
      .mockResolvedValueOnce({
        approval_required: true,
        payload: evmApprovalPayload,
        expires_at: 9999999999,
      })
      .mockResolvedValueOnce({
        approval_required: false,
        payload: evmBurnPayload,
        expires_at: 9999999999,
      });

    const { result } = renderHook(() => useCctpSaga(input));
    await act(async () => {
      await result.current.requestQuote();
    });
    await act(async () => {
      await result.current.prepareSourceBurn();
    });
    expect(result.current.burnPrepareStep).toBe('approval_ready');

    await act(async () => {
      await result.current.signApprovalStep();
    });
    expect(executePreparedPayload).toHaveBeenCalledTimes(1);
    expect(submitBurn).toHaveBeenCalledTimes(1);
    expect(result.current.burnPrepareStep).toBe('unknown');

    await act(async () => {
      await result.current.prepareSourceBurn();
    });
    expect(result.current.burnPrepareStep).toBe('burn_ready');
    const burnFingerprint = fingerprintPreparedPayload(evmBurnPayload);
    expect(result.current.getLastPreparedFingerprint()).toBe(burnFingerprint);

    await act(async () => {
      await result.current.signBurnStep();
    });
    expect(executePreparedPayload).toHaveBeenCalledTimes(2);
    expect(prepareBurn).toHaveBeenCalledTimes(2);
  });

  it('Stellar: server approval_required drives Stellar approval then burn', async () => {
    const input = {
      sourceChainId: 'stellar' as const,
      destChainId: 'ethereum-sepolia' as const,
      amount: '10',
      recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0',
      wallets: {
        recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0',
        sourceStellarAdapterId: 'freighter',
        evmDestinationAdapterId: 'evm:dest',
      },
      bridgeReady: true,
      quoteInputsKey: 'k2',
    };
    prepareBurn
      .mockResolvedValueOnce({
        approval_required: true,
        payload: stellarApprovalPayload,
        expires_at: 9999999999,
      })
      .mockResolvedValueOnce({
        approval_required: false,
        payload: stellarBurnPayload,
        expires_at: 9999999999,
      });

    const { result } = renderHook(() => useCctpSaga(input));
    await act(async () => {
      await result.current.requestQuote();
    });
    await act(async () => {
      await result.current.prepareSourceBurn();
    });
    await act(async () => {
      await result.current.signApprovalStep();
    });
    expect(executePreparedPayload).toHaveBeenCalledTimes(1);
    await act(async () => {
      await result.current.prepareSourceBurn();
    });
    await act(async () => {
      await result.current.signBurnStep();
    });
    expect(executePreparedPayload).toHaveBeenCalledTimes(2);
    const fp1 = fingerprintPreparedPayload(stellarApprovalPayload);
    const fp2 = fingerprintPreparedPayload(stellarBurnPayload);
    expect(fp1).not.toBe(fp2);
  });

  it('does not submit burn when EVM receipt is pending', async () => {
    prepareBurn.mockResolvedValue({
      approval_required: false,
      payload: evmBurnPayload,
      expires_at: 9999999999,
    });
    executePreparedPayload.mockResolvedValue({
      txHash: '0xpending',
      submissionReady: false,
    });
    const { result } = renderHook(() =>
      useCctpSaga({
        sourceChainId: 'ethereum-sepolia',
        destChainId: 'stellar',
        amount: '10',
        recipient: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
        wallets: { recipient: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF', sourceEvmAdapterId: 'evm:test' },
        bridgeReady: true,
        quoteInputsKey: 'k3',
      }),
    );
    await act(async () => {
      await result.current.requestQuote();
    });
    await act(async () => {
      await result.current.prepareSourceBurn();
    });
    await act(async () => {
      await result.current.signBurnStep();
    });
    expect(submitBurn).not.toHaveBeenCalled();
    expect(result.current.stage).toBe('pending_reconcile');
  });
});

describe('useCctpSaga reconcile stability', () => {
  beforeEach(() => {
    sessionStorage.clear();
    prepareBurn.mockReset();
    getTransfer.mockReset();
    startPoll.mockClear();
    getTransfer.mockResolvedValue({
      transfer_id: 't1',
      corridor_id: 'c',
      provider: 'circle-cctp',
      direction: 'evm_to_stellar',
      status: 'burn_prepared',
      retryable: false,
    });
  });

  it('auto-reconciles at most once per session revision (StrictMode safe)', async () => {
    seedSession({ burnPrepareStep: 'approval_ready', fingerprint: 'fp-vault' });

    const wrapper = ({ children }: { children: ReactNode }) =>
      createElement(StrictMode, null, children);

    const { rerender } = renderHook(() => useCctpSaga(baseInput), { wrapper });
    await act(async () => {
      await Promise.resolve();
    });
    rerender();
    await act(async () => {
      await Promise.resolve();
    });

    expect(getTransfer).toHaveBeenCalledTimes(1);
  });

  it('reload without in-memory payload requires re-prepare before Approve CTA', async () => {
    const fp = fingerprintPreparedPayload(evmApprovalPayload);
    seedSession({ burnPrepareStep: 'approval_ready', fingerprint: fp });

    const { result } = renderHook(() => useCctpSaga(baseInput));
    await act(async () => {
      await Promise.resolve();
    });

    expect(result.current.burnPrepareStep).toBe('reprepare_required');
    expect(result.current.primaryAction.label).toBe('Re-prepare transaction');
    expect(result.current.primaryAction.action).toBe('prepare');
  });

  it('reload after approval-ready: re-prepare then approve uses one wallet call', async () => {
    const fp = fingerprintPreparedPayload(evmApprovalPayload);
    seedSession({ burnPrepareStep: 'approval_ready', fingerprint: fp });
    prepareBurn.mockResolvedValue({
      approval_required: true,
      payload: evmApprovalPayload,
      expires_at: 9999999999,
    });

    const { result } = renderHook(() => useCctpSaga(baseInput));
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.burnPrepareStep).toBe('reprepare_required');

    await act(async () => {
      await result.current.prepareSourceBurn();
    });
    expect(result.current.burnPrepareStep).toBe('approval_ready');
    expect(prepareBurn).toHaveBeenCalledTimes(1);

    await act(async () => {
      await result.current.signApprovalStep();
    });
    expect(executePreparedPayload).toHaveBeenCalledTimes(1);
    expect(prepareBurn).toHaveBeenCalledTimes(1);
  });

  it('manual resume always refreshes transfer status', async () => {
    seedSession();
    const { result } = renderHook(() => useCctpSaga(baseInput));
    await act(async () => {
      await Promise.resolve();
    });
    getTransfer.mockClear();

    await act(async () => {
      await result.current.resumeTransfer();
    });
    expect(getTransfer).toHaveBeenCalledTimes(1);
  });
});
