'use client';

import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import type { CctpTraderError } from '@/lib/cctp/errors';
import type { CctpQuoteResponse, CctpTransferStatusResponse } from '@/lib/cctp/types';
import type { CctpSagaStage } from '@/hooks/useCctpSaga';
import type { CctpSessionRecoveryMeta } from '@/lib/cctp/session-vault';
import type { WalletRoleMismatch } from '@/lib/cctp/wallet-role-binding';
import { formatCctpTraderStatus } from '@/lib/cctp/status-copy';
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
  resetLabel?: string;
  bridgeUnavailable?: boolean;
  resumeMismatch?: boolean;
  walletRoleMismatch?: WalletRoleMismatch | null;
  sessionPublic?: {
    transferId: string;
    recovery: CctpSessionRecoveryMeta;
  } | null;
  reattestCooldownUntil?: number | null;
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
  resetLabel = 'Start new quote',
  bridgeUnavailable,
  resumeMismatch,
  walletRoleMismatch,
  sessionPublic,
  reattestCooldownUntil,
  className,
}: CctpExecutionPanelProps) {
  const [cooldownSec, setCooldownSec] = useState(0);

  useEffect(() => {
    if (!reattestCooldownUntil) {
      setCooldownSec(0);
      return;
    }
    const tick = () => {
      const remaining = Math.max(
        0,
        Math.ceil((reattestCooldownUntil - Date.now()) / 1000),
      );
      setCooldownSec(remaining);
    };
    tick();
    const id = setInterval(tick, 1000);
    return () => clearInterval(id);
  }, [reattestCooldownUntil]);

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

      {resumeMismatch && sessionPublic && (
        <div
          className="rounded-xl border border-primary/30 bg-primary/5 px-3 py-2 text-sm space-y-1"
          data-testid="cctp-resume-card"
        >
          <p className="font-medium">Resume prior transfer</p>
          <p className="text-muted-foreground">
            A saved transfer exists for {sessionPublic.recovery.amount} USDC (
            {sessionPublic.recovery.sourceChainId} →{' '}
            {sessionPublic.recovery.destChainId}). Current form inputs differ —
            reconcile with the server before continuing.
          </p>
        </div>
      )}

      {walletRoleMismatch && (
        <div
          className="rounded-xl border border-signal/40 bg-signal/10 px-3 py-2 text-sm space-y-2"
          data-testid="cctp-wallet-recovery-card"
          role="alert"
        >
          <p className="font-medium">Connect the original wallet</p>
          <p className="text-muted-foreground">{walletRoleMismatch.message}</p>
          {(walletRoleMismatch.expectedMasked ||
            walletRoleMismatch.currentMasked) && (
            <dl className="grid gap-1 text-xs font-mono">
              {walletRoleMismatch.expectedMasked && (
                <div>
                  <dt className="text-muted-foreground">Expected</dt>
                  <dd>{walletRoleMismatch.expectedMasked}</dd>
                </div>
              )}
              {walletRoleMismatch.currentMasked && (
                <div>
                  <dt className="text-muted-foreground">Connected</dt>
                  <dd>{walletRoleMismatch.currentMasked}</dd>
                </div>
              )}
            </dl>
          )}
        </div>
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
          Status:{' '}
          <span className="font-medium">
            {formatStatus(stage, transferStatus.status)}
          </span>
          {transferStatus.support_reference_id && (
            <span className="text-muted-foreground">
              {' '}
              · Ref {transferStatus.support_reference_id}
            </span>
          )}
        </p>
      )}

      {cooldownSec > 0 && (
        <p className="text-xs text-muted-foreground" role="status">
          Re-attestation available in {cooldownSec}s
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
        {onReset &&
          (stage === 'failed' ||
            stage === 'completed' ||
            stage === 'awaiting_attestation' ||
            stage === 'sign_mint' ||
            stage === 'resume_pending' ||
            Boolean(transferStatus)) && (
            <Button
              type="button"
              variant="outline"
              className="min-h-11"
              onClick={onReset}
              data-testid="cctp-abandon-cta"
            >
              {resetLabel}
            </Button>
          )}
      </div>
    </section>
  );
}

function formatStatus(stage: CctpSagaStage, status?: string): string {
  return formatCctpTraderStatus(stage, status);
}
