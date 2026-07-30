import { renderHook, act } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { useCctpSaga } from './useCctpSaga';

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
    getTransfer: vi.fn(),
  }),
}));

describe('useCctpSaga', () => {
  beforeEach(() => {
    sessionStorage.clear();
  });

  it('requests quote with stable idempotency key until inputs change', async () => {
    const { result, rerender } = renderHook(
      (props) => useCctpSaga(props),
      {
        initialProps: {
          sourceChainId: 'ethereum-sepolia' as const,
          destChainId: 'stellar' as const,
          amount: '10',
          recipient: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
          wallets: { recipient: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF' },
          bridgeReady: true,
          quoteInputsKey: 'k1',
        },
      },
    );

    await act(async () => {
      await result.current.requestQuote();
    });
    expect(result.current.stage).toBe('quoted');
    expect(result.current.quote?.transfer_id).toBe('t1');

    rerender({
      sourceChainId: 'ethereum-sepolia',
      destChainId: 'stellar',
      amount: '11',
      recipient: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
      wallets: { recipient: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF' },
      bridgeReady: true,
      quoteInputsKey: 'k2',
    });
    expect(result.current.stage).toBe('idle');
  });
});
