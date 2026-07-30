'use client';

import { formatChainPairLabel } from '@/lib/cross-chain/format';
import type { ChainDisplayId } from '@/lib/cross-chain/types';

interface UnsupportedCorridorStateProps {
  sourceChainId: ChainDisplayId;
  destChainId: ChainDisplayId;
  reason?: string;
}

export function UnsupportedCorridorState({
  sourceChainId,
  destChainId,
  reason,
}: UnsupportedCorridorStateProps) {
  const pairLabel = formatChainPairLabel(sourceChainId, destChainId);

  return (
    <div
      role="alert"
      className="rounded-2xl border border-signal/35 bg-signal/8 p-4 space-y-2"
      data-testid="unsupported-corridor-alert"
    >
      <p className="text-sm font-semibold text-foreground">
        {pairLabel} — coming soon
      </p>
      <p className="text-sm text-muted-foreground">
        {reason ??
          'This corridor is visible in the catalog but not executable yet. No quote or destination amount is shown to avoid misleading estimates.'}
      </p>
      <p className="text-xs text-muted-foreground">
        Connect wallets to preview signing readiness, or switch to Stellar native
        for live swaps today.
      </p>
    </div>
  );
}
