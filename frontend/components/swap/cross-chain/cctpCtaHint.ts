import type { RecipientValidationResult } from '@/lib/cross-chain/types';
import type { CctpDirection } from '@/lib/cctp/types';

export interface CctpCtaBlockInput {
  direction: CctpDirection | null;
  sourceAmount: string;
  destRecipientAddress: string;
  useRecipientOverride: boolean;
  recipientOverride: string;
  recipientValidation: RecipientValidationResult;
  bridgeReady: boolean;
  readinessLoading: boolean;
  sagaPrimaryDisabled: boolean;
}

export function resolveDestinationRecipientSetupHint(
  direction: CctpDirection | null,
  destRecipientAddress: string,
  useRecipientOverride: boolean,
): string | null {
  if (direction !== 'stellar_to_evm') return null;
  if (useRecipientOverride || destRecipientAddress) return null;
  return 'Connect ETH Sepolia or enable Destination recipient below and paste a 0x address.';
}

export function resolveCctpCtaHint(input: CctpCtaBlockInput): string | null {
  if (input.readinessLoading) {
    return 'Checking CCTP availability…';
  }
  if (!input.bridgeReady) {
    return 'CCTP is not executable on this API right now.';
  }
  if (
    input.useRecipientOverride &&
    input.recipientOverride.trim() &&
    !input.recipientValidation.valid
  ) {
    return (
      input.recipientValidation.message ?? 'Enter a valid destination address.'
    );
  }
  if (!input.destRecipientAddress) {
    if (input.direction === 'stellar_to_evm') {
      return 'Connect ETH Sepolia or enable Destination recipient and paste a 0x address.';
    }
    if (input.direction === 'evm_to_stellar') {
      return 'Connect a Stellar recipient wallet or enable Destination recipient.';
    }
    return 'Set a destination recipient to continue.';
  }
  if (!input.sourceAmount.trim()) {
    return 'Enter a USDC amount to get a quote.';
  }
  return null;
}

/** Mirrors CrossChainSwapDeck disable rules plus recipient validation from canReview. */
export function isCctpPrimaryActionDisabled(input: CctpCtaBlockInput): boolean {
  if (input.readinessLoading) return true;
  if (input.sagaPrimaryDisabled) return true;
  if (!input.destRecipientAddress) return true;
  if (!input.sourceAmount.trim()) return true;
  if (
    input.useRecipientOverride &&
    input.recipientOverride.trim() &&
    !input.recipientValidation.valid
  ) {
    return true;
  }
  return false;
}
