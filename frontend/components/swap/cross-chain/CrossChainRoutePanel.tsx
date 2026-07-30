'use client';

import { CctpStepRail } from './CctpStepRail';
import { formatChainPairLabel } from '@/lib/cross-chain/format';
import type { ChainDisplayId, CrossChainProtocol } from '@/lib/cross-chain/types';
import { cn } from '@/lib/utils';
import { ArrowRight } from 'lucide-react';

interface CrossChainRoutePanelProps {
  sourceChainId: ChainDisplayId;
  destChainId: ChainDisplayId;
  protocol: CrossChainProtocol;
  executable: boolean;
  className?: string;
}

export function CrossChainRoutePanel({
  sourceChainId,
  destChainId,
  protocol,
  executable,
  className,
}: CrossChainRoutePanelProps) {
  const isCctp = protocol === 'cctp-preview';
  const pairLabel = formatChainPairLabel(sourceChainId, destChainId);

  return (
    <section
      aria-label="Route preview"
      className={cn(
        'space-y-4 rounded-2xl border border-border/40 bg-card/50 p-4 sm:p-5',
        className
      )}
      data-testid="cross-chain-route-panel"
    >
      <div className="space-y-1">
        <p className="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
          Route rail
        </p>
        <h2 className="brand-wordmark text-lg text-foreground">{pairLabel}</h2>
        <p className="text-sm text-muted-foreground">
          {executable
            ? 'Stellar-native execution uses SDEX prepare → sign → submit.'
            : 'Protocol preview — quotes and execution are not available for this corridor yet.'}
        </p>
      </div>

      <div className="flex items-center justify-center gap-3 py-2">
        <HubNode label={sourceChainId === 'stellar' ? 'Stellar hub' : 'Source'} active={sourceChainId === 'stellar'} />
        <ArrowRight className="h-5 w-5 text-primary shrink-0" aria-hidden />
        <HubNode label="Stellar hub" active={destChainId === 'stellar' || sourceChainId === 'stellar'} emphasized />
        <ArrowRight className="h-5 w-5 text-primary shrink-0" aria-hidden />
        <HubNode label={destChainId === 'stellar' ? 'Stellar hub' : 'Destination'} active={destChainId === 'stellar'} />
      </div>

      {isCctp && (
        <div className="space-y-2">
          <p className="text-xs font-semibold text-foreground">CCTP rail</p>
          <CctpStepRail previewOnly={!executable} />
          <dl className="grid gap-2 text-xs sm:grid-cols-2">
            <div className="rounded-lg border border-border/30 bg-muted/20 p-2">
              <dt className="text-muted-foreground">Provider</dt>
              <dd className="font-medium">Circle CCTP (preview)</dd>
            </div>
            <div className="rounded-lg border border-border/30 bg-muted/20 p-2">
              <dt className="text-muted-foreground">Fees &amp; finality</dt>
              <dd className="font-medium text-muted-foreground">
                Estimate unavailable — corridor not live
              </dd>
            </div>
          </dl>
        </div>
      )}

      {protocol === 'stellar-native' && (
        <p className="text-xs text-muted-foreground">
          Same-chain routing aggregates SDEX and Soroban venues via the existing
          Stellar swap path.
        </p>
      )}
    </section>
  );
}

function HubNode({
  label,
  active,
  emphasized = false,
}: {
  label: string;
  active: boolean;
  emphasized?: boolean;
}) {
  return (
    <div
      className={cn(
        'flex min-h-11 min-w-[88px] flex-col items-center justify-center rounded-xl border px-3 py-2 text-center',
        active
          ? emphasized
            ? 'border-primary/50 bg-primary/15 text-foreground'
            : 'border-primary/35 bg-primary/8'
          : 'border-border/40 bg-background/40 text-muted-foreground'
      )}
    >
      <span className="text-[10px] uppercase tracking-wide">Hub</span>
      <span className="text-xs font-semibold">{label}</span>
    </div>
  );
}
