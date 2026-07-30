import { describe, expect, it, vi } from 'vitest';

import { createCircleCctpBridgeProvider } from './circle-cctp';
import { executeCctpFlow, type CctpBackendClient, type WalletSigner } from './circle-cctp-flow';
import { WalletAdapterError } from '../errors';

describe('circle-cctp bridge provider seam', () => {
  const provider = createCircleCctpBridgeProvider();
  const route = {
    sourceChain: 'stellar:testnet',
    destinationChain: 'eip155:11155111',
    sourceAsset: 'usdc',
    destinationAsset: 'usdc',
  };

  it('reports unsupported availability until backend enables corridor', () => {
    const availability = provider.getAvailability(route);
    expect(availability.kind).toBe('unsupported');
    expect(availability.code).toBe('no_backend_route');
  });

  it('quote throws unsupported_capability', async () => {
    await expect(
      provider.quote({
        route,
        amount: '10',
        recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0',
      }),
    ).rejects.toBeInstanceOf(WalletAdapterError);
    await expect(
      provider.quote({
        route,
        amount: '10',
        recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0',
      }),
    ).rejects.toMatchObject({ code: 'unsupported_capability' });
  });

  it('prepare throws unsupported_capability', async () => {
    await expect(
      provider.prepare({
        route,
        quoteId: 'q1',
        sender: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
      }),
    ).rejects.toMatchObject({ code: 'unsupported_capability' });
  });

  it('submit returns not_implemented without fake success', async () => {
    const result = await provider.submit({
      route,
      preparedId: 'p1',
      txHash: '0xabc',
    });
    expect(result.status).toBe('not_implemented');
    expect(result).not.toHaveProperty('success', true);
  });
});

describe('circle-cctp flow orchestration (unexported provider deps)', () => {
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

  it('runs approval then burn with correct wallet chain', async () => {
    const prepareBurn = vi
      .fn()
      .mockResolvedValueOnce({
        transferId: 't1',
        approvalRequired: true,
        payload: { step: 'approval', data: 'approve-data' },
      })
      .mockResolvedValueOnce({
        transferId: 't1',
        approvalRequired: false,
        payload: { step: 'burn', data: 'burn-data' },
      });
    const backend = mockBackend({
      isCorridorExecutable: () => true,
      quote: vi.fn().mockResolvedValue({ quoteId: 't1' }),
      prepareBurn,
      submitApproval: vi.fn().mockResolvedValue(undefined),
      submitBurn: vi.fn().mockResolvedValue(undefined),
      pollStatus: vi.fn().mockResolvedValue({ status: 'awaiting_attestation' }),
    });
    const signAndSend = vi.fn().mockResolvedValue('0xtx');
    const result = await executeCctpFlow({
      backend,
      wallet: { chainId: 'eip155:11155111', signAndSend },
      corridorId: 'cctp-testnet',
      amount: '10',
      recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0',
    });
    expect(result.ok).toBe(true);
    expect(prepareBurn).toHaveBeenCalledTimes(2);
    expect(backend.submitApproval).toHaveBeenCalledWith({ transferId: 't1', txHash: '0xtx' });
    expect(signAndSend).toHaveBeenNthCalledWith(1, {
      chainId: 'eip155:11155111',
      data: 'approve-data',
    });
    expect(signAndSend).toHaveBeenNthCalledWith(2, {
      chainId: 'eip155:11155111',
      data: 'burn-data',
    });
  });

  it('reuses prepared burn payload on retry without second prepare', async () => {
    const backend = mockBackend({
      isCorridorExecutable: () => true,
      quote: vi.fn().mockResolvedValue({ quoteId: 't1' }),
      prepareBurn: vi.fn().mockResolvedValue({
        transferId: 't1',
        approvalRequired: false,
        payload: { step: 'burn', data: 'new-burn' },
      }),
      submitBurn: vi.fn().mockResolvedValue(undefined),
      pollStatus: vi.fn().mockResolvedValue({ status: 'awaiting_attestation' }),
    });
    const signAndSend = vi.fn().mockResolvedValue('0xretry');
    await executeCctpFlow({
      backend,
      wallet: { chainId: 'eip155:11155111', signAndSend },
      corridorId: 'cctp-testnet',
      amount: '10',
      recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0',
      preparedBurnPayload: 'cached-burn',
    });
    expect(signAndSend).toHaveBeenCalledWith({
      chainId: 'eip155:11155111',
      data: 'cached-burn',
    });
  });

  it('mint retry path does not call quote or burn submit', async () => {
    const backend = mockBackend({
      isCorridorExecutable: () => true,
      quote: vi.fn(),
      prepareMint: vi.fn().mockResolvedValue({ payloadHash: 'h1', payload: 'mint-data' }),
      submitMint: vi.fn().mockResolvedValue({ status: 'completed' }),
      submitBurn: vi.fn(),
    });
    const signAndSend = vi.fn().mockResolvedValue('0xmint');
    const result = await executeCctpFlow({
      backend,
      wallet: { chainId: 'eip155:11155111', signAndSend },
      corridorId: 'cctp-testnet',
      amount: '10',
      recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0',
      transferId: 't1',
      mintRetryOnly: true,
    });
    expect(result).toEqual({ ok: true, transferId: 't1', finalStatus: 'completed' });
    expect(backend.quote).not.toHaveBeenCalled();
    expect(backend.submitBurn).not.toHaveBeenCalled();
    expect(backend.submitMint).toHaveBeenCalledWith({
      transferId: 't1',
      txHash: '0xmint',
      payloadHash: 'h1',
    });
  });

  it('surfaces ambiguous errors for missing burn payload', async () => {
    const backend = mockBackend({
      isCorridorExecutable: () => true,
      quote: vi.fn().mockResolvedValue({ quoteId: 't1' }),
      prepareBurn: vi.fn().mockResolvedValue({
        transferId: 't1',
        approvalRequired: false,
      }),
    });
    const result = await executeCctpFlow({
      backend,
      wallet,
      corridorId: 'cctp-testnet',
      amount: '10',
      recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0',
    });
    expect(result).toEqual({
      ok: false,
      code: 'ambiguous_error',
      message: 'missing burn payload',
    });
  });
});
