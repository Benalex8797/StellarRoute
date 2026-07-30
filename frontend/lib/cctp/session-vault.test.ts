import { describe, expect, it, vi, beforeEach } from 'vitest';
import {
  patchCctpSessionRecovery,
  setPendingEvmTx,
  loadCctpSession,
  buildCctpSessionRecord,
  clearCctpSession,
} from './session-vault';

describe('session-vault pending EVM tx', () => {
  beforeEach(() => {
    sessionStorage.clear();
  });

  it('persists pending tx hash without access token in recovery', () => {
    const record = buildCctpSessionRecord({
      transferId: 't1',
      accessToken: 'secret-token',
      idempotencyKey: 'k1',
      recovery: {
        corridorId: 'c1',
        direction: 'evm_to_stellar',
        sourceChainId: 'ethereum-sepolia',
        destChainId: 'stellar',
        amount: '10',
        recipient: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
      },
    });
    save(record);
    setPendingEvmTx({ txHash: '0xabc', purpose: 'burn' });
    const loaded = loadCctpSession();
    expect(loaded.ok).toBe(true);
    if (loaded.ok) {
      expect(loaded.record.recovery.pendingEvmTx?.txHash).toBe('0xabc');
      expect(loaded.record.recovery).not.toHaveProperty('accessToken');
    }
  });

  it('clears expired pending tx on load', () => {
    const record = buildCctpSessionRecord({
      transferId: 't1',
      accessToken: 'secret-token',
      idempotencyKey: 'k1',
      recovery: {
        corridorId: 'c1',
        direction: 'evm_to_stellar',
        sourceChainId: 'ethereum-sepolia',
        destChainId: 'stellar',
        amount: '10',
        recipient: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
        pendingEvmTx: {
          txHash: '0xold',
          purpose: 'burn',
          expiresAt: Date.now() - 1000,
        },
      },
    });
    save(record);
    const loaded = loadCctpSession();
    expect(loaded.ok).toBe(true);
    if (loaded.ok) {
      expect(loaded.record.recovery.pendingEvmTx).toBeUndefined();
    }
  });
});

function save(record: ReturnType<typeof buildCctpSessionRecord>) {
  sessionStorage.setItem('stellarroute:cctp:v1', JSON.stringify(record));
}

describe('patchCctpSessionRecovery', () => {
  beforeEach(() => {
    clearCctpSession();
    sessionStorage.clear();
  });

  it('updates burn prepare step', () => {
    const record = buildCctpSessionRecord({
      transferId: 't1',
      accessToken: 'tok',
      idempotencyKey: 'k1',
      recovery: {
        corridorId: 'c1',
        direction: 'stellar_to_evm',
        sourceChainId: 'stellar',
        destChainId: 'ethereum-sepolia',
        amount: '5',
        recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0',
      },
    });
    save(record);
    const patched = patchCctpSessionRecovery({ burnPrepareStep: 'approval_ready' });
    expect(patched?.recovery.burnPrepareStep).toBe('approval_ready');
  });
});
