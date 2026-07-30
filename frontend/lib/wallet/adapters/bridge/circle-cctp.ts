import { WalletAdapterError } from '../errors';
import type { ExecutionSupport } from '../types';
import type {
  BridgeExecutionProvider,
  BridgePrepareRequest,
  BridgePreparedPayload,
  BridgeQuote,
  BridgeQuoteRequest,
  BridgeRouteHint,
  BridgeSubmitRequest,
  BridgeSubmitResult,
} from './types';

function notExecutable(route: BridgeRouteHint): ExecutionSupport {
  return {
    kind: 'unsupported',
    code: 'no_backend_route',
    message: `Circle CCTP is not executable for ${route.sourceChain} → ${route.destinationChain} until backend corridor enablement.`,
    resolution: 'Wait for backend health to list this corridor as executable',
  };
}

/** Circle CCTP provider seam — non-executable until backend lists corridor. */
export function createCircleCctpBridgeProvider(
  id = 'circle-cctp',
  label = 'Circle CCTP',
): BridgeExecutionProvider {
  return {
    id,
    label,
    getAvailability(route) {
      return notExecutable(route);
    },
    async quote(request: BridgeQuoteRequest): Promise<BridgeQuote> {
      void request;
      throw new WalletAdapterError(
        'CCTP quote unavailable until corridor is executable',
        'unsupported_capability',
        id,
      );
    },
    async prepare(request: BridgePrepareRequest): Promise<BridgePreparedPayload> {
      void request;
      throw new WalletAdapterError(
        'CCTP prepare unavailable until corridor is executable',
        'unsupported_capability',
        id,
      );
    },
    async submit(request: BridgeSubmitRequest): Promise<BridgeSubmitResult> {
      void request;
      return {
        status: 'not_implemented',
        message: 'CCTP submit disabled until backend corridor is executable',
      };
    },
  };
}
