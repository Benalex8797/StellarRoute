'use client';

import { cn } from '@/lib/utils';
import type { ChainDisplayId } from '@/lib/cross-chain/types';
import { CHAIN_DEFINITIONS } from '@/lib/cross-chain/corridors';
import type { WalletChipBinding } from '@/lib/cross-chain/wallet-chip-types';
import { ChainWalletChip } from './ChainWalletChip';
import type { CrossChainWalletStoryState } from './crossChainStoryPresentation';

interface PairedChainSelectorsProps {
  sourceChainId: ChainDisplayId;
  destChainId: ChainDisplayId;
  onSourceChange: (id: ChainDisplayId) => void;
  onDestChange: (id: ChainDisplayId) => void;
  sourceWalletState?: CrossChainWalletStoryState;
  destWalletState?: CrossChainWalletStoryState;
  sourceWalletBinding?: WalletChipBinding | null;
  destWalletBinding?: WalletChipBinding | null;
  mintSubmitterBinding?: WalletChipBinding | null;
  inputsLocked?: boolean;
}

export function PairedChainSelectors({
  sourceChainId,
  destChainId,
  onSourceChange,
  onDestChange,
  sourceWalletState,
  destWalletState,
  sourceWalletBinding,
  destWalletBinding,
  mintSubmitterBinding,
  inputsLocked = false,
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
      {inputsLocked && (
        <p
          className="text-xs text-muted-foreground"
          role="status"
          data-testid="cctp-inputs-locked-banner"
        >
          Transfer in progress — chain and amount inputs are locked. Wallets stay
          connectable for signing. Use Start new transfer to abandon.
        </p>
      )}
      <div className="grid gap-4 sm:grid-cols-2">
        <ChainLegColumn
          role="source"
          chainId={sourceChainId}
          onChange={onSourceChange}
          walletStoryState={sourceWalletState}
          walletBinding={sourceWalletBinding}
          inputsLocked={inputsLocked}
        />
        <ChainLegColumn
          role="destination"
          chainId={destChainId}
          onChange={onDestChange}
          walletStoryState={destWalletState}
          walletBinding={destWalletBinding}
          inputsLocked={inputsLocked}
        />
      </div>
      {mintSubmitterBinding && (
        <div
          className="rounded-xl border border-border/40 bg-muted/10 p-3 space-y-2"
          data-testid="stellar-mint-submitter-control"
        >
          <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-primary">
            Stellar mint submitter / fee payer
          </p>
          <p className="text-xs text-muted-foreground">
            Muxed recipients cannot sign. Connect a Stellar G account to submit
            the mint transaction.
          </p>
          <ChainWalletChip binding={mintSubmitterBinding} />
        </div>
      )}
    </section>
  );
}

function ChainLegColumn({
  role,
  chainId,
  onChange,
  walletStoryState,
  walletBinding,
  inputsLocked,
}: {
  role: 'source' | 'destination';
  chainId: ChainDisplayId;
  onChange: (id: ChainDisplayId) => void;
  walletStoryState?: CrossChainWalletStoryState;
  walletBinding?: WalletChipBinding | null;
  inputsLocked?: boolean;
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
        <ChainWalletChip
          binding={walletBinding}
          storyState={walletStoryState}
        />
      </div>
      <ChainSelector
        value={chainId}
        onChange={onChange}
        label={`${role === 'source' ? 'Source' : 'Destination'} chain`}
        name={`cross-chain-${role}`}
        role={role}
        disabled={inputsLocked}
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
  disabled,
}: {
  value: ChainDisplayId;
  onChange: (id: ChainDisplayId) => void;
  label: string;
  name: string;
  role: 'source' | 'destination';
  disabled?: boolean;
}) {
  return (
    <fieldset className="space-y-2" disabled={disabled}>
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
                'relative inline-flex min-h-11 min-w-[4.5rem] cursor-pointer items-center rounded-xl border px-3 py-2 transition-colors',
                'has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-ring',
                selected
                  ? 'border-primary/50 bg-primary/12 text-foreground'
                  : 'border-border/50 bg-background/50 text-muted-foreground hover:bg-muted/40',
                disabled && 'pointer-events-none opacity-60',
              )}
            >
              <input
                type="radio"
                name={name}
                value={id}
                checked={selected}
                disabled={disabled}
                onChange={() => onChange(id)}
                className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
                data-testid={`chain-option-${role}-${id}`}
              />
              <span className="relative z-[1] text-xs font-semibold pointer-events-none">
                {chain.shortLabel}
              </span>
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}
