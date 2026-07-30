import { describe, expect, it, vi } from 'vitest';
import { executeCctpFlow, type CctpBackendClient, type WalletSigner } from './circle-cctp-flow';

describe('circle-cctp flow orchestration', () => {
  function mockBackend(overrides: Partial<CctpBackendClient> = {}): CctpBackendClient {
    return {
      isCorridorExecutable: () => false,
      quote: vi.fn(),
      prepareBurn: vi.fn(),
      submitApproval: vi.fn(),
      submitBurn: vi.fn(),
      pollStatus: vi.fn(),
      prepareMint: vi.fn(),
      submitMint: vi.fn(),
      ...overrides,
    };
  }

  const wallet: WalletSigner = {
    chainId: 'eip155:11155111',
    signAndSend: vi.fn().mockResolvedValue('0xsigned'),
  };

  it('returns backend_unavailable when corridor absent', async () => {
    const result = await executeCctpFlow({
      backend: mockBackend({ isCorridorExecutable: () => false }),
      wallet,
      corridorId: 'cctp-testnet',
      amount: '10',
      recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0',
    });
    expect(result).toEqual({
      ok: false,
      code: 'backend_unavailable',
      message: 'corridor not executable',
    });
  });
});
