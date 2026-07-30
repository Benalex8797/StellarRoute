'use client';

import { Button } from '@/components/ui/button';
import type { CctpTraderError } from '@/lib/cctp/errors';
import type { CctpQuoteResponse, CctpTransferStatusResponse } from '@/lib/cctp/types';
import type { CctpSagaStage } from '@/hooks/useCctpSaga';
import { cn } from '@/lib/utils';

interface CctpExecutionPanelProps {
  stage: CctpSagaStage;
  quote: CctpQuoteResponse | null;
  transferStatus: CctpTransferStatusResponse | null;
  error: CctpTraderError | null;
  primaryLabel: string;
  primaryDisabled: boolean;
  onPrimary: () => void;
  onReset?: () => void;
  bridgeUnavailable?: boolean;
  className?: string;
}

export function CctpExecutionPanel({
  stage,
  quote,
  transferStatus,
  error,
  primaryLabel,
  primaryDisabled,
  onPrimary,
  onReset,
  bridgeUnavailable,
  className,
}: CctpExecutionPanelProps) {
  return (
    <section
      className={cn(
        'space-y-3 rounded-2xl border border-border/40 bg-card/40 p-4',
        className,
      )}
      aria-label="CCTP transfer execution"
      data-testid="cctp-execution-panel"
    >
      {bridgeUnavailable && (
        <p className="text-sm text-muted-foreground" role="status">
          CCTP is not executable on this API right now. Check status and retry
          when the corridor shows live.
        </p>
      )}

      {quote && (
        <dl className="grid gap-2 text-xs sm:grid-cols-2">
          <div className="rounded-lg border border-border/30 bg-muted/20 p-2">
            <dt className="text-muted-foreground">You send</dt>
            <dd className="font-medium">{quote.source_amount} USDC</dd>
          </div>
          <div className="rounded-lg border border-border/30 bg-muted/20 p-2">
            <dt className="text-muted-foreground">You receive</dt>
            <dd className="font-medium">{quote.destination_amount} USDC</dd>
          </div>
          <div className="rounded-lg border border-border/30 bg-muted/20 p-2">
            <dt className="text-muted-foreground">Finality</dt>
            <dd className="font-medium capitalize">{quote.finality}</dd>
          </div>
          <div className="rounded-lg border border-border/30 bg-muted/20 p-2">
            <dt className="text-muted-foreground">Quote expires</dt>
            <dd className="font-medium">
              {new Date(quote.expires_at * 1000).toLocaleTimeString()}
            </dd>
          </div>
        </dl>
      )}

      {transferStatus && (
        <p className="text-sm" role="status" data-testid="cctp-saga-status">
          Status: <span className="font-medium">{formatStatus(stage, transferStatus.status)}</span>
          {transferStatus.support_reference_id && (
            <span className="text-muted-foreground">
              {' '}
              · Ref {transferStatus.support_reference_id}
            </span>
          )}
        </p>
      )}

      {error && (
        <div
          className="rounded-xl border border-signal/30 bg-signal/10 px-3 py-2 text-sm"
          role="alert"
          data-testid="cctp-error-banner"
        >
          <p className="font-medium">{error.title}</p>
          <p className="text-muted-foreground">{error.message}</p>
        </div>
      )}

      <div className="flex flex-wrap gap-2">
        <Button
          type="button"
          className="min-h-11"
          disabled={primaryDisabled}
          onClick={onPrimary}
          data-testid="cross-chain-review-cta"
        >
          {primaryLabel}
        </Button>
        {(stage === 'failed' || stage === 'completed') && onReset && (
          <Button type="button" variant="outline" className="min-h-11" onClick={onReset}>
            Start new quote
          </Button>
        )}
      </div>
    </section>
  );
}

function formatStatus(stage: CctpSagaStage, status?: string): string {
  if (stage === 'completed' || status === 'completed') return 'Complete';
  if (status) return status.replace(/_/g, ' ');
  return stage.replace(/_/g, ' ');
}
