'use client';

import { cn } from '@/lib/utils';
import type { ChainDisplayId } from '@/lib/cross-chain/types';
import { CHAIN_DEFINITIONS } from '@/lib/cross-chain/corridors';
import { ChainWalletChip } from './ChainWalletChip';
import type { CrossChainWalletStoryState } from './crossChainStoryPresentation';

interface PairedChainSelectorsProps {
  sourceChainId: ChainDisplayId;
  destChainId: ChainDisplayId;
  onSourceChange: (id: ChainDisplayId) => void;
  onDestChange: (id: ChainDisplayId) => void;
  sourceWalletState?: CrossChainWalletStoryState;
  destWalletState?: CrossChainWalletStoryState;
}

export function PairedChainSelectors({
  sourceChainId,
  destChainId,
  onSourceChange,
  onDestChange,
  sourceWalletState,
  destWalletState,
}: PairedChainSelectorsProps) {
  return (
    <section
      aria-label="Source and destination chains"
      className="rounded-2xl border border-border/40 bg-card/50 p-4 sm:p-5 space-y-4"
      data-testid="paired-chain-selectors"
    >
      <p className="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
        Route legs
      </p>
      <div className="grid gap-4 sm:grid-cols-2">
        <ChainLegColumn
          role="source"
          chainId={sourceChainId}
          onChange={onSourceChange}
          walletStoryState={sourceWalletState}
        />
        <ChainLegColumn
          role="destination"
          chainId={destChainId}
          onChange={onDestChange}
          walletStoryState={destWalletState}
        />
      </div>
    </section>
  );
}

function ChainLegColumn({
  role,
  chainId,
  onChange,
  walletStoryState,
}: {
  role: 'source' | 'destination';
  chainId: ChainDisplayId;
  onChange: (id: ChainDisplayId) => void;
  walletStoryState?: CrossChainWalletStoryState;
}) {
  const chain = CHAIN_DEFINITIONS[chainId];
  const legLabel = role === 'source' ? 'You send from' : 'You receive on';

  return (
    <div className="space-y-3" data-testid={`chain-leg-${role}`}>
      <div className="flex items-start justify-between gap-2">
        <div>
          <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-primary">
            {legLabel}
          </p>
          <p className="text-sm font-semibold text-foreground">{chain.label}</p>
        </div>
        <ChainWalletChip chain={chain} storyState={walletStoryState} />
      </div>
      <ChainSelector
        value={chainId}
        onChange={onChange}
        label={`${role === 'source' ? 'Source' : 'Destination'} chain`}
        name={`cross-chain-${role}`}
        role={role}
      />
    </div>
  );
}

const CHAIN_ORDER: ChainDisplayId[] = [
  'stellar',
  'ethereum-sepolia',
  'solana',
  'bitcoin',
  'tron',
];

function ChainSelector({
  value,
  onChange,
  label,
  name,
  role,
}: {
  value: ChainDisplayId;
  onChange: (id: ChainDisplayId) => void;
  label: string;
  name: string;
  role: 'source' | 'destination';
}) {
  return (
    <fieldset className="space-y-2">
      <legend className="font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground">
        {label}
      </legend>
      <div
        role="radiogroup"
        aria-label={label}
        className="flex flex-wrap gap-1.5"
      >
        {CHAIN_ORDER.map((id) => {
          const chain = CHAIN_DEFINITIONS[id];
          const selected = value === id;
          return (
            <label
              key={id}
              className={cn(
                'inline-flex min-h-11 cursor-pointer items-center rounded-xl border px-3 py-2 transition-colors',
                'has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-ring',
                selected
                  ? 'border-primary/50 bg-primary/12 text-foreground'
                  : 'border-border/50 bg-background/50 text-muted-foreground hover:bg-muted/40'
              )}
            >
              <input
                type="radio"
                name={name}
                value={id}
                checked={selected}
                onChange={() => onChange(id)}
                className="sr-only"
                data-testid={`chain-option-${role}-${id}`}
              />
              <span className="text-xs font-semibold">{chain.shortLabel}</span>
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}
