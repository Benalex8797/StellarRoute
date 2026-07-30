import { StellarRouteApiError } from '@/lib/api/client';
import { isUserRejection, WalletAdapterError } from '@/lib/wallet/adapters';

export type CctpErrorKind =
  | 'retryable'
  | 'nonretryable'
  | 'wallet_rejection'
  | 'wrong_network'
  | 'quote_expired'
  | 'payload_expired'
  | 'provider_killed'
  | 'dependency_unavailable'
  | 'authorization_lost'
  | 'pending_ambiguous';

export interface CctpTraderError {
  kind: CctpErrorKind;
  title: string;
  message: string;
  requestId?: string;
  action?: string;
}

export function mapCctpError(err: unknown): CctpTraderError {
  const message = err instanceof Error ? err.message : '';
  if (
    (message && isUserRejection(message)) ||
    isWalletRejection(err)
  ) {
    return {
      kind: 'wallet_rejection',
      title: 'Signature cancelled',
      message: 'You declined the wallet request. Nothing was submitted.',
      action: 'Try again when ready',
    };
  }

  if (err instanceof WalletAdapterError) {
    if (err.code === 'network_mismatch') {
      return {
        kind: 'wrong_network',
        title: 'Wrong network',
        message:
          'Switch your wallet to the required network, then try again.',
        action: 'Switch network in wallet',
      };
    }
  }

  if (err instanceof StellarRouteApiError) {
    const requestId = extractRequestId(err.details);
    switch (err.code) {
      case 'cctp_not_enabled':
      case 'dependency_unavailable':
        return {
          kind: 'dependency_unavailable',
          title: 'Bridge temporarily unavailable',
          message:
            'CCTP is not ready on this deployment. Wait a moment and refresh readiness.',
          requestId,
          action: 'Retry',
        };
      case 'provider_killed':
        return {
          kind: 'provider_killed',
          title: 'Provider paused',
          message:
            'Circle CCTP is temporarily disabled. Signing is blocked until service recovers.',
          requestId,
          action: 'Check status',
        };
      case 'transfer_not_found':
        return {
          kind: 'authorization_lost',
          title: 'Transfer authorization lost',
          message:
            'This transfer cannot be resumed without its access token. Start a new quote.',
          requestId,
          action: 'New quote',
        };
      case 'quote_expired':
        return {
          kind: 'quote_expired',
          title: 'Quote expired',
          message: 'Request a fresh quote before signing.',
          requestId,
          action: 'Refresh quote',
        };
      case 'payload_expired':
        return {
          kind: 'payload_expired',
          title: 'Payload expired',
          message: 'Prepare a new wallet payload before signing again.',
          requestId,
          action: 'Prepare again',
        };
      case 'attestation_pending':
        return {
          kind: 'pending_ambiguous',
          title: 'Attestation in progress',
          message: 'Circle is still attesting your burn. This can take a few minutes.',
          requestId,
        };
      default:
        if (err.status === 503) {
          return {
            kind: 'dependency_unavailable',
            title: 'Service unavailable',
            message: 'The bridge API is temporarily unavailable. Try again shortly.',
            requestId,
            action: 'Retry',
          };
        }
        if (err.status >= 500) {
          return {
            kind: 'retryable',
            title: 'Temporary error',
            message: 'Something went wrong on our side. You can retry safely.',
            requestId,
            action: 'Retry',
          };
        }
        return {
          kind: 'nonretryable',
          title: 'Transfer blocked',
          message: err.message || 'This transfer cannot continue.',
          requestId,
        };
    }
  }

  if (err instanceof Error) {
    if (/expired/i.test(err.message)) {
      return {
        kind: 'payload_expired',
        title: 'Payload expired',
        message: err.message,
        action: 'Prepare again',
      };
    }
    if (/timeout|ambiguous|pending/i.test(err.message)) {
      return {
        kind: 'pending_ambiguous',
        title: 'Submission uncertain',
        message: err.message,
        action: 'Check explorer',
      };
    }
    return {
      kind: 'nonretryable',
      title: 'Something went wrong',
      message: err.message || 'Unknown error',
    };
  }

  return {
    kind: 'nonretryable',
    title: 'Something went wrong',
    message: 'Unknown error',
  };
}

function isWalletRejection(err: unknown): boolean {
  if (!(err instanceof Error)) return false;
  const code = (err as Error & { code?: string }).code;
  return code === 'user_rejected' || /reject|denied|cancel/i.test(err.message);
}

function extractRequestId(details: unknown): string | undefined {
  if (!details || typeof details !== 'object') return undefined;
  const id = (details as { request_id?: unknown }).request_id;
  return typeof id === 'string' ? id : undefined;
}
