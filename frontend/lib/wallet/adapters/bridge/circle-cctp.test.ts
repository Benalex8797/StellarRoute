import { describe, expect, it } from 'vitest';

import { createCircleCctpBridgeProvider } from './circle-cctp';
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
