'use client';

import { cn } from '@/lib/utils';
import type { ChainDisplayId } from '@/lib/cross-chain/types';
import { CHAIN_DEFINITIONS } from '@/lib/cross-chain/corridors';

interface ChainSelectorProps {
  value: ChainDisplayId;
  onChange: (id: ChainDisplayId) => void;
  label: string;
  disabled?: boolean;
  'aria-label'?: string;
}

const CHAIN_ORDER: ChainDisplayId[] = [
  'stellar',
  'ethereum-sepolia',
  'solana',
  'bitcoin',
  'tron',
];

export function ChainSelector({
  value,
  onChange,
  label,
  disabled = false,
  'aria-label': ariaLabel,
}: ChainSelectorProps) {
  return (
    <div className="space-y-1.5">
      <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground">
        {label}
      </span>
      <div
        role="listbox"
        aria-label={ariaLabel ?? label}
        className="flex flex-wrap gap-1.5"
      >
        {CHAIN_ORDER.map((id) => {
          const chain = CHAIN_DEFINITIONS[id];
          const selected = value === id;
          return (
            <button
              key={id}
              type="button"
              role="option"
              aria-selected={selected}
              disabled={disabled}
              onClick={() => onChange(id)}
              className={cn(
                'min-h-11 min-w-[44px] rounded-xl border px-3 py-2 text-left transition-colors',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
                selected
                  ? 'border-primary/50 bg-primary/12 text-foreground'
                  : 'border-border/50 bg-background/50 text-muted-foreground hover:bg-muted/40',
                disabled && 'pointer-events-none opacity-50'
              )}
              data-testid={`chain-option-${id}`}
            >
              <span className="block text-xs font-semibold">{chain.shortLabel}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
