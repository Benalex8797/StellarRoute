import { describe, expect, it, vi } from 'vitest';
import { executeEvmPreparedPayload } from './evm-execution';
import { SEPOLIA_CHAIN_ID } from './types';
import { SEPOLIA_CCTP_CONTRACTS } from './constants';

describe('executeEvmPreparedPayload', () => {
  const payload = {
    type: 'evm_transaction' as const,
    chain_id: SEPOLIA_CHAIN_ID,
    to: SEPOLIA_CCTP_CONTRACTS.usdc,
    data: '0x',
    value: '0',
  };

  it('switches network before send when chain mismatches', async () => {
    const switchNetwork = vi.fn().mockResolvedValue(undefined);
    const sendTransaction = vi.fn().mockResolvedValue({
      kind: 'evm_transaction' as const,
      hash: '0xabc',
    });
    await executeEvmPreparedPayload({
      payload,
      evmAdapterId: 'evm:injected',
      deps: {
        readChainIdHex: vi
          .fn()
          .mockResolvedValueOnce('0x1')
          .mockResolvedValueOnce('0xaa36a7'),
        switchNetwork,
        sendTransaction,
        waitForReceipt: vi.fn().mockResolvedValue('success'),
      },
    });
    expect(switchNetwork).toHaveBeenCalledWith(
      'evm:injected',
      'eip155:11155111',
    );
    expect(sendTransaction).toHaveBeenCalledWith(
      'evm:injected',
      expect.objectContaining({
        transaction: expect.objectContaining({ chainId: '0xaa36a7' }),
      }),
    );
  });

  it('maps user rejection on network switch', async () => {
    const switchNetwork = vi.fn().mockRejectedValue(
      Object.assign(new Error('User rejected'), { code: 'user_rejected' }),
    );
    await expect(
      executeEvmPreparedPayload({
        payload,
        evmAdapterId: 'evm:injected',
        deps: {
          readChainIdHex: vi.fn().mockResolvedValue('0x1'),
          switchNetwork,
          sendTransaction: vi.fn(),
          waitForReceipt: vi.fn(),
        },
      }),
    ).rejects.toMatchObject({ code: 'user_rejected' });
  });
});
