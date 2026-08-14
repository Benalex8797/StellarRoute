import type { CctpTransferStatus } from '@/lib/cctp/types';
import type { CctpSagaStage } from '@/hooks/useCctpSaga';

const STATUS_LABELS: Record<string, string> = {
  created: 'Created',
  burn_prepared: 'Ready to burn',
  burn_submitted: 'Burn submitted',
  // Circle Standard Transfer from Ethereum/Sepolia waits ~65 blocks (~15–19 min).
  // EVM→Stellar quotes use Fast by default (~seconds).
  awaiting_attestation: 'Awaiting Circle attestation',
  attestation_ready: 'Attestation ready',
  mint_prepared: 'Ready to mint',
  mint_submitted: 'Mint submitted',
  completed: 'Complete',
  attestation_failed: 'Attestation failed',
  mint_failed_retryable: 'Mint failed — retryable',
  cancelled: 'Cancelled',
  provider_killed: 'Provider paused',
};

export function formatCctpTraderStatus(
  stage: CctpSagaStage,
  status?: CctpTransferStatus | string,
): string {
  if (stage === 'completed' || status === 'completed') return 'Complete';
  if (stage === 'pending_reconcile') return 'Transaction pending';
  if (status && STATUS_LABELS[status]) return STATUS_LABELS[status];
  if (stage === 'sign_approval') return 'Approve USDC spend';
  if (stage === 'sign_burn') return 'Sign burn';
  if (stage === 'sign_trustline') return 'Open USDC trustline';
  if (stage === 'sign_mint') return 'Sign mint';
  if (stage === 'quoted') return 'Quote ready';
  return stage.replace(/_/g, ' ');
}
