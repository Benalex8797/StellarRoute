import type { ChainFamily } from '@/lib/wallet/adapters';
import type { RecipientValidationResult } from './types';

const STELLAR_ADDRESS = /^G[ABCDEFGHIJKLMNOPQRSTUVWXYZ234567]{55}$/;
const EVM_ADDRESS = /^0x[a-fA-F0-9]{40}$/;
const SOLANA_ADDRESS = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/;
const BITCOIN_ADDRESS =
  /^(?:bc1|tb1)[a-z0-9]{25,87}$|^[13][a-km-zA-HJ-NP-Z1-9]{25,34}$/;
const TRON_ADDRESS = /^T[1-9A-HJ-NP-Za-km-z]{33}$/;

export function validateRecipientAddress(
  chainFamily: ChainFamily,
  value: string
): RecipientValidationResult {
  const trimmed = value.trim();
  if (!trimmed) {
    return { valid: false, message: 'Recipient address is required.' };
  }

  switch (chainFamily) {
    case 'stellar':
      return STELLAR_ADDRESS.test(trimmed)
        ? { valid: true }
        : {
            valid: false,
            message: 'Enter a valid Stellar address (G…, 56 characters).',
          };
    case 'evm':
      return EVM_ADDRESS.test(trimmed)
        ? { valid: true }
        : {
            valid: false,
            message: 'Enter a valid EVM address (0x + 40 hex digits).',
          };
    case 'solana':
      return SOLANA_ADDRESS.test(trimmed)
        ? { valid: true }
        : {
            valid: false,
            message: 'Enter a valid Solana address (base58, 32–44 chars).',
          };
    case 'bitcoin':
      return BITCOIN_ADDRESS.test(trimmed)
        ? { valid: true }
        : {
            valid: false,
            message:
              'Enter a valid Bitcoin address (bc1…, 1…, or 3… format).',
          };
    case 'tron':
      return TRON_ADDRESS.test(trimmed)
        ? { valid: true }
        : {
            valid: false,
            message: 'Enter a valid TRON address (T…, 34 characters).',
          };
    default:
      return { valid: false, message: 'Unsupported chain for recipient.' };
  }
}
