'use client';

import { cn } from '@/lib/utils';

const STEPS = [
  { id: 'burn', label: 'Burn', detail: 'Lock USDC on source chain' },
  { id: 'attest', label: 'Attest', detail: 'Circle attestation relay' },
  { id: 'mint', label: 'Mint', detail: 'Release on destination' },
] as const;

interface CctpStepRailProps {
  previewOnly?: boolean;
  className?: string;
}

export function CctpStepRail({ previewOnly = true, className }: CctpStepRailProps) {
  return (
    <ol
      className={cn('flex flex-col gap-2 sm:flex-row sm:items-stretch', className)}
      aria-label="CCTP protocol steps"
    >
      {STEPS.map((step, index) => (
        <li
          key={step.id}
          className={cn(
            'relative flex min-h-11 flex-1 flex-col rounded-xl border border-border/40 bg-background/50 p-3',
            previewOnly && 'border-dashed'
          )}
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
      ))}
    </ol>
  );
}
