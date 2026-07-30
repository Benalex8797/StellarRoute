import { StrKey } from '@stellar/stellar-base';
import { describe, expect, it } from 'vitest';
import {
  assessWalletRoleBindings,
  buildWalletRoleBindings,
  classifyStellarRecipient,
  normalizeEvmAddress,
  normalizeStellarGAddress,
} from './wallet-role-binding';

const STELLAR_G = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';
const STELLAR_M = 'MAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABUQSUK';
const EVM_SOURCE = '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0';
const EVM_OTHER = '0x1111111111111111111111111111111111111111';

describe('wallet-role-binding normalization', () => {
  it('normalizes EVM addresses case-insensitively', () => {
    expect(normalizeEvmAddress(EVM_SOURCE)).toBe(EVM_SOURCE.toLowerCase());
    expect(normalizeEvmAddress(EVM_SOURCE.toUpperCase())).toBeNull();
    expect(
      normalizeEvmAddress('0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0'),
    ).toBe(EVM_SOURCE.toLowerCase());
    expect(normalizeEvmAddress('0xshort')).toBeNull();
  });

  it('accepts exact Stellar G and classifies M recipients', () => {
    expect(normalizeStellarGAddress(STELLAR_G)).toBe(STELLAR_G);
    expect(normalizeStellarGAddress(STELLAR_G.toLowerCase())).toBeNull();
    expect(classifyStellarRecipient(STELLAR_G)).toBe('stellar_g');
    expect(classifyStellarRecipient(STELLAR_M)).toBe('stellar_m');
  });
});

describe('buildWalletRoleBindings', () => {
  it('binds EVM source + Stellar mint submitter for evm_to_stellar', () => {
    const bindings = buildWalletRoleBindings({
      direction: 'evm_to_stellar',
      sourceChainId: 'eip155:11155111',
      destChainId: 'stellar:testnet',
      sender: EVM_SOURCE,
      recipient: STELLAR_G,
      mintSubmitter: STELLAR_G,
    });
    expect(bindings?.sourceBurn.adapterFamily).toBe('evm');
    expect(bindings?.stellarMintSubmitter?.address).toBe(STELLAR_G);
    expect(bindings?.evmMintSubmitter).toBeUndefined();
  });

  it('marks stellar_to_evm EVM mint as permissionless', () => {
    const bindings = buildWalletRoleBindings({
      direction: 'stellar_to_evm',
      sourceChainId: 'stellar:testnet',
      destChainId: 'eip155:11155111',
      sender: STELLAR_G,
      recipient: EVM_SOURCE,
    });
    expect(bindings?.evmMintSubmitter?.mode).toBe('permissionless');
    expect(bindings?.stellarMintSubmitter).toBeUndefined();
  });
});

describe('assessWalletRoleBindings', () => {
  const evmBindings = buildWalletRoleBindings({
    direction: 'evm_to_stellar',
    sourceChainId: 'eip155:11155111',
    destChainId: 'stellar:testnet',
    sender: EVM_SOURCE,
    recipient: STELLAR_G,
    mintSubmitter: STELLAR_G,
  })!;

  it('rejects missing bindings (old schema)', () => {
    const result = assessWalletRoleBindings({
      bindings: undefined,
      wallets: {
        recipient: STELLAR_G,
        sourceEvmAdapterId: 'evm:test',
        sourceAddress: EVM_SOURCE,
        mintSubmitter: STELLAR_G,
        mintSubmitterStellarAdapterId: 'freighter',
      },
      intent: 'source_approval',
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.issue.code).toBe('bindings_missing');
  });

  it('rejects wrong EVM source account', () => {
    const result = assessWalletRoleBindings({
      bindings: evmBindings,
      wallets: {
        recipient: STELLAR_G,
        sourceEvmAdapterId: 'evm:test',
        sourceAddress: EVM_OTHER,
        mintSubmitter: STELLAR_G,
      },
      intent: 'source_burn',
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.issue.code).toBe('source_burn_mismatch');
  });

  it('accepts checksum-mixed EVM source', () => {
    const result = assessWalletRoleBindings({
      bindings: evmBindings,
      wallets: {
        recipient: STELLAR_G,
        sourceEvmAdapterId: 'evm:test',
        sourceAddress: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0',
        mintSubmitter: STELLAR_G,
        mintSubmitterStellarAdapterId: 'freighter',
      },
      intent: 'source_approval',
    });
    expect(result.ok).toBe(true);
  });

  it('requires correct Stellar G mint submitter with M recipient', () => {
    const muxedBindings = buildWalletRoleBindings({
      direction: 'evm_to_stellar',
      sourceChainId: 'eip155:11155111',
      destChainId: 'stellar:testnet',
      sender: EVM_SOURCE,
      recipient: STELLAR_M,
      mintSubmitter: STELLAR_G,
    })!;
    const wrong = assessWalletRoleBindings({
      bindings: muxedBindings,
      wallets: {
        recipient: STELLAR_M,
        sourceEvmAdapterId: 'evm:test',
        sourceAddress: EVM_SOURCE,
        mintSubmitter: 'GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB',
        mintSubmitterStellarAdapterId: 'freighter',
      },
      intent: 'stellar_mint',
    });
    expect(wrong.ok).toBe(false);

    const ok = assessWalletRoleBindings({
      bindings: muxedBindings,
      wallets: {
        recipient: STELLAR_M,
        sourceEvmAdapterId: 'evm:test',
        sourceAddress: EVM_SOURCE,
        mintSubmitter: STELLAR_G,
        mintSubmitterStellarAdapterId: 'freighter',
      },
      intent: 'stellar_mint',
    });
    expect(ok.ok).toBe(true);
  });

  it('rejects missing EVM adapter for source burn', () => {
    const result = assessWalletRoleBindings({
      bindings: evmBindings,
      wallets: {
        recipient: STELLAR_G,
        sourceAddress: EVM_SOURCE,
      },
      intent: 'source_approval',
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.issue.code).toBe('source_adapter_missing');
  });
});
