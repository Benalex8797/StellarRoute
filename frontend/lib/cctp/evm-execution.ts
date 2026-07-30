import {
  getAdapter,
  sendWithChainWallet,
  type AdapterNetworkId,
  type SendTransactionRequest,
} from '@/lib/wallet/adapters';
import { WalletAdapterError } from '@/lib/wallet/adapters';
import { caip2ToChainIdHex } from '@/lib/wallet/adapters/evm/networks';
import { assertSepoliaCaip, caip2FromChainIdHex } from './caip-evm';
import type { PreparedWalletPayload } from './types';
import { validatePreparedPayload } from './payload-validation';

export const DEFAULT_RECEIPT_POLL_MS = 2_000;
export const DEFAULT_RECEIPT_TIMEOUT_MS = 120_000;
export const MAX_EVM_CALLDATA_BYTES = 24_576;

export type EvmSendDeps = {
  sendTransaction: (
    adapterId: string,
    request: SendTransactionRequest,
  ) => Promise<{ kind: 'evm_transaction'; hash: string }>;
  switchNetwork?: (
    adapterId: string,
    network: AdapterNetworkId,
  ) => Promise<void>;
  readChainIdHex?: (adapterId: string) => Promise<string | null>;
  waitForReceipt?: (
    txHash: string,
    opts: { signal?: AbortSignal; timeoutMs?: number },
  ) => Promise<'success' | 'reverted' | 'timeout' | 'dropped'>;
};

const defaultDeps: EvmSendDeps = {
  sendTransaction: async (adapterId, request) => {
    const result = await sendWithChainWallet(adapterId, request);
    if (result.kind !== 'evm_transaction' || !result.hash) {
      throw new Error('EVM wallet did not return a transaction hash.');
    }
    return { kind: 'evm_transaction', hash: result.hash };
  },
  switchNetwork: async (adapterId, network) => {
    const adapter = getAdapter(adapterId);
    if (!adapter?.switchNetwork) {
      throw new WalletAdapterError(
        'Wallet cannot switch networks automatically.',
        'unsupported_capability',
        adapterId,
      );
    }
    await adapter.switchNetwork(network);
  },
  readChainIdHex: async (adapterId) => {
    const adapter = getAdapter(adapterId);
    if (!adapter) return null;
    const info = await adapter.getNetwork();
    return caip2ToChainIdHex(info.network);
  },
  waitForReceipt: waitForSepoliaReceipt,
};

export async function executeEvmPreparedPayload(input: {
  payload: Extract<PreparedWalletPayload, { type: 'evm_transaction' }>;
  evmAdapterId: string;
  expiresAtSec?: number;
  deps?: Partial<EvmSendDeps>;
  signal?: AbortSignal;
}): Promise<{ txHash: string; receiptStatus: 'success' | 'timeout' | 'dropped' }> {
  const deps = { ...defaultDeps, ...input.deps };
  const validation = validatePreparedPayload(input.payload, {
    expiresAtSec: input.expiresAtSec,
  });
  if (!validation.ok) {
    const err = new Error(validation.message) as Error & { code: string };
    err.code = validation.code;
    throw err;
  }

  const parsed = assertSepoliaCaip(input.payload.chain_id);
  if (!parsed.ok) {
    const err = new Error(parsed.message) as Error & { code: string };
    err.code = parsed.code;
    throw err;
  }

  const expectedNetwork = caip2FromChainIdHex(parsed.chainIdHex);
  const currentHex = await deps.readChainIdHex!(input.evmAdapterId);
  if (currentHex?.toLowerCase() !== parsed.chainIdHex.toLowerCase()) {
    try {
      await deps.switchNetwork!(input.evmAdapterId, expectedNetwork);
    } catch (switchErr) {
      if (isUserRejected(switchErr)) {
        const err = new Error('Network switch declined in wallet.') as Error & {
          code: string;
        };
        err.code = 'user_rejected';
        throw err;
      }
      throw switchErr;
    }
    const afterHex = await deps.readChainIdHex!(input.evmAdapterId);
    if (afterHex?.toLowerCase() !== parsed.chainIdHex.toLowerCase()) {
      throw new WalletAdapterError(
        'Wallet network does not match Sepolia after switch.',
        'network_mismatch',
        input.evmAdapterId,
      );
    }
  }

  const valueHex = normalizeEvmValueHex(input.payload.value);
  const sendReq: SendTransactionRequest = {
    kind: 'evm_transaction',
    transaction: {
      chainId: parsed.chainIdHex,
      to: input.payload.to,
      data: input.payload.data,
      value: valueHex,
      gas: input.payload.gas,
      gasPrice: input.payload.gas_price,
      maxFeePerGas: input.payload.max_fee_per_gas,
      maxPriorityFeePerGas: input.payload.max_priority_fee_per_gas,
    },
  };

  const sent = await deps.sendTransaction(input.evmAdapterId, sendReq);
  const receiptStatus = await deps.waitForReceipt!(sent.hash, {
    signal: input.signal,
    timeoutMs: DEFAULT_RECEIPT_TIMEOUT_MS,
  });

  if (receiptStatus === 'dropped') {
    const err = new Error(
      'Transaction was not mined within the timeout. It may have been dropped or replaced — reconcile via explorer before resubmitting.',
    ) as Error & { code: string };
    err.code = 'pending_ambiguous';
    throw err;
  }

  if (receiptStatus === 'reverted') {
    throw new Error('EVM transaction reverted on-chain.');
  }

  return {
    txHash: sent.hash,
    receiptStatus: receiptStatus === 'success' ? 'success' : 'timeout',
  };
}

function normalizeEvmValueHex(value: string): string {
  if (value.startsWith('0x')) return value;
  const asBig = BigInt(value);
  return `0x${asBig.toString(16)}`;
}

function isUserRejected(err: unknown): boolean {
  if (err instanceof WalletAdapterError && err.code === 'user_rejected') {
    return true;
  }
  if (err instanceof Error && /reject|denied|cancel/i.test(err.message)) {
    return true;
  }
  return false;
}

/** Poll Sepolia receipt via public RPC. Replacement detection is not available via EIP-1193 alone. */
export async function waitForSepoliaReceipt(
  txHash: string,
  opts: { signal?: AbortSignal; timeoutMs?: number; pollMs?: number } = {},
): Promise<'success' | 'reverted' | 'timeout' | 'dropped'> {
  const timeoutMs = opts.timeoutMs ?? DEFAULT_RECEIPT_TIMEOUT_MS;
  const pollMs = opts.pollMs ?? DEFAULT_RECEIPT_POLL_MS;
  const rpcUrl = 'https://ethereum-sepolia.publicnode.com';
  const started = Date.now();

  while (Date.now() - started < timeoutMs) {
    if (opts.signal?.aborted) return 'timeout';
    try {
      const response = await fetch(rpcUrl, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          jsonrpc: '2.0',
          id: 1,
          method: 'eth_getTransactionReceipt',
          params: [txHash],
        }),
        signal: opts.signal,
      });
      if (response.ok) {
        const body = (await response.json()) as {
          result?: { status?: string } | null;
        };
        if (body.result) {
          const status = body.result.status ?? '0x1';
          return status === '0x0' ? 'reverted' : 'success';
        }
      }
    } catch {
      // keep polling until timeout
    }
    await sleep(pollMs, opts.signal);
  }
  return 'dropped';
}

function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal?.aborted) {
      resolve();
      return;
    }
    const timer = setTimeout(resolve, ms);
    signal?.addEventListener(
      'abort',
      () => {
        clearTimeout(timer);
        resolve();
      },
      { once: true },
    );
  });
}
