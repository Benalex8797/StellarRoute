'use client';

import { cn } from '@/lib/utils';
import type { CctpQuoteResponse } from '@/lib/cctp/types';

const STEPS = [
  { id: 'burn', label: 'Burn', detail: 'Lock USDC on source chain' },
  { id: 'attest', label: 'Attest', detail: 'Circle attestation relay' },
  { id: 'mint', label: 'Mint', detail: 'Release on destination' },
] as const;

export type CctpStepId = (typeof STEPS)[number]['id'];

interface CctpStepRailProps {
  previewOnly?: boolean;
  activeStep?: CctpStepId | null;
  className?: string;
}

export function CctpStepRail({
  previewOnly = true,
  activeStep = null,
  className,
}: CctpStepRailProps) {
  return (
    <ol
      className={cn('flex flex-col gap-2 sm:flex-row sm:items-stretch', className)}
      aria-label="CCTP protocol steps"
    >
      {STEPS.map((step, index) => {
        const isActive = !previewOnly && activeStep === step.id;
        return (
          <li
            key={step.id}
            className={cn(
              'relative flex min-h-11 flex-1 flex-col rounded-xl border p-3',
              previewOnly
                ? 'border-dashed border-border/40 bg-background/50'
                : isActive
                  ? 'border-primary/50 bg-primary/10'
                  : 'border-border/40 bg-background/50',
            )}
            aria-current={isActive ? 'step' : undefined}
          >
            <span className="font-mono text-[10px] uppercase tracking-wider text-primary">
              Step {index + 1}
            </span>
            <span className="text-sm font-semibold">{step.label}</span>
            <span className="text-xs text-muted-foreground">{step.detail}</span>
            {previewOnly && (
              <span className="mt-1 text-[10px] uppercase tracking-wide text-muted-foreground">
                Preview — not live
              </span>
            )}
          </li>
        );
      })}
    </ol>
  );
}

export function cctpActiveStepFromSaga(
  status?: string,
): CctpStepId | null {
  if (!status) return 'burn';
  if (status === 'completed') return 'mint';
  if (
    status === 'awaiting_attestation' ||
    status === 'attestation_ready' ||
    status === 'attestation_failed'
  ) {
    return 'attest';
  }
  if (
    status === 'mint_prepared' ||
    status === 'mint_submitted' ||
    status === 'mint_failed_retryable'
  ) {
    return 'mint';
  }
  return 'burn';
}
