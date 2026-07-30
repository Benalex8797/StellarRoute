import {
  sendWithChainWallet,
  signWithChainWallet,
  type SendTransactionRequest,
  type SignTransactionRequest,
} from '@/lib/wallet/adapters';
import { submitToHorizon } from '@/lib/wallet/submit';
import type { WalletNetwork } from '@/lib/wallet/types';
import type { PreparedWalletPayload } from './types';
import { validatePreparedPayload } from './payload-validation';

export interface WalletExecutionResult {
  txHash: string;
}

export async function executePreparedPayload(input: {
  payload: PreparedWalletPayload;
  stellarAdapterId?: string;
  evmAdapterId?: string;
  walletNetwork?: WalletNetwork | null;
  expiresAtSec?: number;
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
        input.walletNetwork ?? 'testnet',
      );
      if (recovered) return { txHash: recovered };
      throw submitErr;
    }
  }

  if (!input.evmAdapterId) {
    throw new Error('Connect an EVM wallet on Sepolia to sign.');
  }
  const sendReq: SendTransactionRequest = {
    kind: 'evm_transaction',
    transaction: {
      chainId: input.payload.chain_id,
      to: input.payload.to,
      data: input.payload.data,
      value: input.payload.value ?? '0x0',
      gas: input.payload.gas,
      gasPrice: input.payload.gas_price,
      maxFeePerGas: input.payload.max_fee_per_gas,
      maxPriorityFeePerGas: input.payload.max_priority_fee_per_gas,
    },
  };
  const sent = await sendWithChainWallet(input.evmAdapterId, sendReq);
  if (sent.kind !== 'evm_transaction' || !sent.hash) {
    throw new Error('EVM wallet did not return a transaction hash.');
  }
  return { txHash: sent.hash };
}

async function recoverHorizonByHash(
  signedXdr: string,
  network: WalletNetwork | null,
): Promise<string | null> {
  try {
    const { TransactionBuilder } = await import('@stellar/stellar-base');
    const tx = TransactionBuilder.fromXDR(
      signedXdr,
      'Test SDF Network ; September 2015',
    );
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
    // recovery failed
  }
  return null;
}
