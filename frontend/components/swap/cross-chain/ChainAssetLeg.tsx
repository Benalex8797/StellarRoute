'use client';

import { AmountInput } from '@/components/swap/AmountInput';
import { ChainSelector } from './ChainSelector';
import { ChainWalletChip } from './ChainWalletChip';
import type { ChainDefinition } from '@/lib/cross-chain/types';
import type { ChainDisplayId } from '@/lib/cross-chain/types';

interface ChainAssetLegProps {
  role: 'source' | 'destination';
  chain: ChainDefinition;
  chainId: ChainDisplayId;
  onChainChange: (id: ChainDisplayId) => void;
  amount?: string;
  onAmountChange?: (value: string) => void;
  amountReadOnly?: boolean;
  amountDisabled?: boolean;
  walletStoryState?: 'disconnected' | 'connecting' | 'connected' | 'mismatch' | 'unsupported';
  chainSelectorDisabled?: boolean;
}

export function ChainAssetLeg({
  role,
  chain,
  chainId,
  onChainChange,
  amount = '',
  onAmountChange,
  amountReadOnly = false,
  amountDisabled = false,
  walletStoryState,
  chainSelectorDisabled = false,
}: ChainAssetLegProps) {
  const legLabel = role === 'source' ? 'You send' : 'You receive';

  return (
    <section
      aria-label={`${legLabel} on ${chain.label}`}
      className="space-y-4 rounded-2xl border border-border/40 bg-card/60 p-4 sm:p-5"
      data-testid={`chain-leg-${role}`}
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="font-mono text-[10px] uppercase tracking-[0.16em] text-primary">
            {legLabel}
          </p>
          <h3 className="brand-wordmark text-lg text-foreground">{chain.assetLabel}</h3>
        </div>
        <ChainWalletChip chain={chain} storyState={walletStoryState} />
      </div>

      <ChainSelector
        label={`${role === 'source' ? 'Source' : 'Destination'} chain`}
        value={chainId}
        onChange={onChainChange}
        disabled={chainSelectorDisabled}
        aria-label={`Select ${role} chain`}
      />

      <AmountInput
        label="Amount"
        value={amount}
        onChange={onAmountChange}
        readOnly={amountReadOnly}
        disabled={amountDisabled}
        placeholder={role === 'destination' && amountReadOnly ? '—' : '0.00'}
        assetId={chain.defaultAssetId}
        showMax={role === 'source' && !amountReadOnly && !amountDisabled}
      />
    </section>
  );
}
