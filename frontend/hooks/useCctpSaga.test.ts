import { renderHook, act } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { useCctpSaga } from './useCctpSaga';
import { fingerprintPreparedPayload } from '@/lib/cctp/payload-fingerprint';

const prepareBurn = vi.fn();
const submitBurn = vi.fn();
const getTransfer = vi.fn();
const executePreparedPayload = vi.fn();

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

describe('useCctpSaga server-driven burn staging', () => {
  beforeEach(() => {
    sessionStorage.clear();
    prepareBurn.mockReset();
    submitBurn.mockReset();
    getTransfer.mockReset();
    executePreparedPayload.mockReset();
    executePreparedPayload.mockResolvedValue({ txHash: '0xhash', submissionReady: true });
    submitBurn.mockResolvedValue({ status: 'burn_submitted' });
    getTransfer.mockResolvedValue({ status: 'burn_prepared', retryable: false });
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
