import { renderHook, act } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { useCctpSaga } from './useCctpSaga';

const prepareBurn = vi.fn();
const submitBurn = vi.fn();
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
    getTransfer: vi.fn(),
  }),
}));

vi.mock('@/lib/cctp/wallet-execution', () => ({
  executePreparedPayload: (...args: unknown[]) => executePreparedPayload(...args),
}));

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
  evmSourceBurn: true,
};

describe('useCctpSaga approval/burn CTAs', () => {
  beforeEach(() => {
    sessionStorage.clear();
    prepareBurn.mockReset();
    submitBurn.mockReset();
    executePreparedPayload.mockReset();
    executePreparedPayload.mockResolvedValue({ txHash: '0xhash' });
    submitBurn.mockResolvedValue({ status: 'burn_submitted' });
  });

  it('approval CTA signs exactly once and stops', async () => {
    prepareBurn.mockResolvedValueOnce({
      approval_required: true,
      payload: { type: 'evm_transaction' },
      expires_at: 9999999999,
    });
    const { result } = renderHook(() => useCctpSaga(baseInput));

    await act(async () => {
      await result.current.requestQuote();
    });
    await act(async () => {
      await result.current.signApprovalStep();
    });

    expect(executePreparedPayload).toHaveBeenCalledTimes(1);
    expect(submitBurn).toHaveBeenCalledTimes(1);
    expect(prepareBurn).toHaveBeenCalledTimes(1);
  });

  it('burn CTA uses fresh prepare and one wallet call', async () => {
    prepareBurn.mockResolvedValueOnce({
      approval_required: false,
      payload: { type: 'evm_transaction' },
      expires_at: 9999999999,
    });
    const { result } = renderHook(() => useCctpSaga(baseInput));

    await act(async () => {
      await result.current.requestQuote();
    });
    await act(async () => {
      await result.current.signBurnStep();
    });

    expect(executePreparedPayload).toHaveBeenCalledTimes(1);
    expect(prepareBurn).toHaveBeenCalledTimes(1);
  });
});
