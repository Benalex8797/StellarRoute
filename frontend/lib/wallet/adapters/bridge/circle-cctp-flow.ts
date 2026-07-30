/**
 * Testable CCTP client flow orchestration (provider seam stays non-executable).
 * Injected deps allow unit tests without registering the provider.
 */

export type CctpBackendClient = {
  isCorridorExecutable: (corridorId: string) => boolean;
  quote: (req: { corridorId: string; amount: string; recipient: string }) => Promise<{ quoteId: string }>;
  prepareBurn: (req: { transferId: string }) => Promise<{
    transferId: string;
    approvalRequired: boolean;
    payload?: { step: 'approval' | 'burn'; data: string };
    payloadHash?: string;
  }>;
  submitApproval: (req: { transferId: string; txHash: string }) => Promise<void>;
  submitBurn: (req: { transferId: string; txHash: string }) => Promise<void>;
  pollStatus: (transferId: string) => Promise<{ status: string }>;
  prepareMint: (transferId: string) => Promise<{ payloadHash: string; payload: string }>;
  submitMint: (req: { transferId: string; txHash: string; payloadHash: string }) => Promise<{ status: string }>;
};

export type WalletSigner = {
  chainId: string;
  signAndSend: (payload: { chainId: string; data: string }) => Promise<string>;
};

export type CctpFlowResult =
  | { ok: true; transferId: string; finalStatus: string }
  | { ok: false; code: 'backend_unavailable' | 'ambiguous_error'; message: string };

export async function executeCctpFlow(deps: {
  backend: CctpBackendClient;
  wallet: WalletSigner;
  corridorId: string;
  amount: string;
  recipient: string;
  transferId?: string;
  /** When set, reuse prepared burn payload (idempotent retry). */
  preparedBurnPayload?: string;
  /** Mint retry only — never auto-reburn. */
  mintRetryOnly?: boolean;
}): Promise<CctpFlowResult> {
  if (!deps.backend.isCorridorExecutable(deps.corridorId)) {
    return { ok: false, code: 'backend_unavailable', message: 'corridor not executable' };
  }

  if (deps.mintRetryOnly) {
    if (!deps.transferId) {
      return { ok: false, code: 'ambiguous_error', message: 'mint retry requires transferId' };
    }
    const mintPrep = await deps.backend.prepareMint(deps.transferId);
    if (deps.wallet.chainId !== 'eip155:11155111' && deps.wallet.chainId !== 'stellar:testnet') {
      return { ok: false, code: 'ambiguous_error', message: 'wrong wallet chain' };
    }
    const mintTx = await deps.wallet.signAndSend({
      chainId: deps.wallet.chainId,
      data: mintPrep.payload,
    });
    const mintResult = await deps.backend.submitMint({
      transferId: deps.transferId,
      txHash: mintTx,
      payloadHash: mintPrep.payloadHash,
    });
    return { ok: true, transferId: deps.transferId, finalStatus: mintResult.status };
  }

  const quote = await deps.backend.quote({
    corridorId: deps.corridorId,
    amount: deps.amount,
    recipient: deps.recipient,
  });

  const transferId = deps.transferId ?? quote.quoteId;
  const prepared = await deps.backend.prepareBurn({ transferId });

  if (prepared.approvalRequired && !deps.preparedBurnPayload) {
    if (!prepared.payload || prepared.payload.step !== 'approval') {
      return { ok: false, code: 'ambiguous_error', message: 'expected approval payload' };
    }
    const approvalTx = await deps.wallet.signAndSend({
      chainId: deps.wallet.chainId,
      data: prepared.payload.data,
    });
    await deps.backend.submitApproval({ transferId, txHash: approvalTx });
    const burnPrep = await deps.backend.prepareBurn({ transferId });
    if (!burnPrep.payload || burnPrep.payload.step !== 'burn') {
      return { ok: false, code: 'ambiguous_error', message: 'expected burn payload after approval' };
    }
    const burnData = deps.preparedBurnPayload ?? burnPrep.payload.data;
    const burnTx = await deps.wallet.signAndSend({ chainId: deps.wallet.chainId, data: burnData });
    await deps.backend.submitBurn({ transferId, txHash: burnTx });
  } else {
    const burnData = deps.preparedBurnPayload ?? prepared.payload?.data;
    if (!burnData) {
      return { ok: false, code: 'ambiguous_error', message: 'missing burn payload' };
    }
    const burnTx = await deps.wallet.signAndSend({ chainId: deps.wallet.chainId, data: burnData });
    await deps.backend.submitBurn({ transferId, txHash: burnTx });
  }

  const polled = await deps.backend.pollStatus(transferId);
  return { ok: true, transferId, finalStatus: polled.status };
}
