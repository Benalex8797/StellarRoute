import {
  signWithChainWallet,
  type SignTransactionRequest,
} from '@/lib/wallet/adapters';
import { submitToHorizon } from '@/lib/wallet/submit';
import type { WalletNetwork } from '@/lib/wallet/types';
import type { PreparedWalletPayload } from './types';
import { validatePreparedPayload } from './payload-validation';
import { executeEvmPreparedPayload } from './evm-execution';

export interface WalletExecutionResult {
  txHash: string;
}

export async function executePreparedPayload(input: {
  payload: PreparedWalletPayload;
  stellarAdapterId?: string;
  evmAdapterId?: string;
  walletNetwork?: WalletNetwork | null;
  expiresAtSec?: number;
  signal?: AbortSignal;
}): Promise<WalletExecutionResult> {
  const validation = validatePreparedPayload(input.payload, {
    expiresAtSec: input.expiresAtSec,
  });
  if (!validation.ok) {
    const err = new Error(validation.message) as Error & { code: string };
    err.code = validation.code;
    throw err;
  }

  if (input.payload.type === 'stellar_xdr') {
    if (!input.stellarAdapterId) {
      throw new Error('Connect a Stellar wallet to sign.');
    }
    const signReq: SignTransactionRequest = {
      kind: 'stellar_xdr',
      xdr: input.payload.xdr_envelope,
      networkPassphrase: input.payload.network_passphrase,
    };
    const signed = await signWithChainWallet(input.stellarAdapterId, signReq);
    if (signed.kind !== 'stellar_xdr' || !signed.signedXdr?.trim()) {
      throw new Error('Wallet returned an empty signed envelope.');
    }
    try {
      const result = await submitToHorizon(
        signed.signedXdr,
        input.walletNetwork ?? 'testnet',
      );
      return { txHash: result.hash };
    } catch (submitErr) {
      const recovered = await recoverHorizonByHash(
        signed.signedXdr,
        input.payload.network_passphrase,
        input.walletNetwork ?? 'testnet',
      );
      if (recovered) return { txHash: recovered };
      throw submitErr;
    }
  }

  if (!input.evmAdapterId) {
    throw new Error('Connect an EVM wallet on Sepolia to sign.');
  }
  const { txHash } = await executeEvmPreparedPayload({
    payload: input.payload,
    evmAdapterId: input.evmAdapterId,
    expiresAtSec: input.expiresAtSec,
    signal: input.signal,
  });
  return { txHash };
}

async function recoverHorizonByHash(
  signedXdr: string,
  networkPassphrase: string,
  network: WalletNetwork | null,
): Promise<string | null> {
  try {
    const { TransactionBuilder } = await import('@stellar/stellar-base');
    const tx = TransactionBuilder.fromXDR(signedXdr, networkPassphrase);
    const hash = tx.hash().toString('hex');
    const { getHorizonUrl } = await import('@/lib/wallet/submit');
    const horizonUrl = getHorizonUrl(network);
    const response = await fetch(
      `${horizonUrl}/transactions/${encodeURIComponent(hash)}`,
    );
    if (response.ok) {
      const body = (await response.json()) as { hash?: string };
      return body.hash ?? hash;
    }
  } catch {
    // recovery failed — do not resubmit blindly
  }
  return null;
}
