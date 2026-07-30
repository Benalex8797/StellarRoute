import type { PreparedWalletPayload } from './types';
import {
  SEPOLIA_CHAIN_ID,
  STELLAR_TESTNET_PASSPHRASE,
} from './types';
import { SEPOLIA_CCTP_CONTRACTS } from './constants';

export type PayloadValidationResult =
  | { ok: true }
  | { ok: false; code: string; message: string };

export function validatePreparedPayload(
  payload: PreparedWalletPayload,
  opts: {
    expectedStellarPassphrase?: string;
    expectedEvmChainId?: string;
    nowSec?: number;
    expiresAtSec?: number;
  } = {},
): PayloadValidationResult {
  const nowSec = opts.nowSec ?? Math.floor(Date.now() / 1000);
  if (opts.expiresAtSec !== undefined && nowSec >= opts.expiresAtSec) {
    return {
      ok: false,
      code: 'payload_expired',
      message: 'Wallet payload expired. Prepare again before signing.',
    };
  }

  if (payload.type === 'stellar_xdr') {
    const expected =
      opts.expectedStellarPassphrase ?? STELLAR_TESTNET_PASSPHRASE;
    if (payload.network_passphrase !== expected) {
      return {
        ok: false,
        code: 'network_mismatch',
        message: 'Prepared Stellar network does not match this app.',
      };
    }
    if (!payload.xdr_envelope?.trim()) {
      return {
        ok: false,
        code: 'validation_error',
        message: 'Prepared Stellar envelope is empty.',
      };
    }
    return { ok: true };
  }

  const expectedChain = opts.expectedEvmChainId ?? SEPOLIA_CHAIN_ID;
  if (payload.chain_id !== expectedChain) {
    return {
      ok: false,
      code: 'network_mismatch',
      message: 'Prepared EVM chain does not match Sepolia testnet.',
    };
  }

  const allowedTo = new Set<string>([
    SEPOLIA_CCTP_CONTRACTS.tokenMessenger,
    SEPOLIA_CCTP_CONTRACTS.messageTransmitter,
    SEPOLIA_CCTP_CONTRACTS.usdc,
  ]);
  if (!allowedTo.has(payload.to.toLowerCase())) {
    return {
      ok: false,
      code: 'validation_error',
      message: 'Prepared EVM target contract is not allowlisted.',
    };
  }

  if (!/^0x[a-fA-F0-9]*$/.test(payload.data)) {
    return {
      ok: false,
      code: 'validation_error',
      message: 'Prepared EVM calldata is malformed.',
    };
  }

  if (!/^\d+$/.test(payload.value ?? '0')) {
    return {
      ok: false,
      code: 'validation_error',
      message: 'Prepared EVM value is malformed.',
    };
  }

  return { ok: true };
}
