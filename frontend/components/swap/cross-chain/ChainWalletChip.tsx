'use client';

import { useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useWallet } from '@/components/providers/wallet-provider';
import { useChainWallet } from '@/hooks/useChainWallet';
import { shortenAddress } from '@/lib/cross-chain/format';
import type { ChainDefinition } from '@/lib/cross-chain/types';
import { cn } from '@/lib/utils';
import { Loader2, Plug, Unplug, Wallet } from 'lucide-react';
import type { CrossChainWalletStoryState } from './crossChainStoryPresentation';

interface ChainWalletChipProps {
  chain: ChainDefinition;
  storyState?: CrossChainWalletStoryState;
  className?: string;
}

export function ChainWalletChip({
  chain,
  storyState,
  className,
}: ChainWalletChipProps) {
  if (chain.chainFamily === 'stellar') {
    return (
      <StellarWalletChip className={className} storyState={storyState} />
    );
  }
  return (
    <ExternalChainWalletChip
      chain={chain}
      storyState={storyState}
      className={className}
    />
  );
}

function ExternalChainWalletChip({
  chain,
  storyState,
  className,
}: ChainWalletChipProps) {
  const live = useChainWallet({
    chainFamily: chain.chainFamily,
    expectedNetwork: chain.networkId,
  });
  const [pickerOpen, setPickerOpen] = useState(false);

  const isConnecting = storyState === 'connecting' || live.isLoading;
  const isConnected =
    storyState === 'connected' || (storyState === undefined && live.isConnected);
  const networkMismatch =
    storyState === 'mismatch' ||
    (storyState === undefined && live.networkMismatch);
  const unsupported = storyState === 'unsupported';

  const statusLabel = unsupported
    ? 'Unsupported wallet'
    : isConnecting
      ? 'Connecting'
      : networkMismatch
        ? 'Network mismatch'
        : isConnected
          ? 'Connected'
          : 'Disconnected';

  return (
    <>
      <button
        type="button"
        onClick={() => setPickerOpen(true)}
        className={cn(
          'flex min-h-11 items-center gap-2 rounded-xl border px-3 py-2 text-left transition-colors',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
          networkMismatch
            ? 'border-signal/40 bg-signal/10'
            : isConnected
              ? 'border-primary/35 bg-primary/10'
              : 'border-border/50 bg-background/40',
          className
        )}
        aria-label={`${chain.shortLabel} wallet: ${statusLabel}`}
        data-testid={`wallet-chip-${chain.id}`}
      >
        <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-muted/50">
          {isConnecting ? (
            <Loader2 className="h-4 w-4 animate-spin" aria-hidden />
          ) : (
            <Wallet className="h-4 w-4" aria-hidden />
          )}
        </span>
        <span className="min-w-0">
          <span className="block text-[10px] uppercase tracking-wide text-muted-foreground">
            {statusLabel}
          </span>
          <span className="block truncate font-mono text-xs font-semibold">
            {isConnected && live.address
              ? shortenAddress(live.address)
              : chain.shortLabel}
          </span>
        </span>
      </button>

      <Dialog open={pickerOpen} onOpenChange={setPickerOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{chain.label} wallet</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            {networkMismatch && (
              <p role="alert" className="text-sm text-signal">
                Wallet network does not match {chain.label}. Switch networks before
                signing.
              </p>
            )}
            {live.availableWallets.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                No {chain.shortLabel} browser wallet detected. Install a supported
                extension to connect.
              </p>
            ) : (
              <ul className="space-y-2">
                {live.availableWallets.map((wallet) => (
                  <li key={wallet.id}>
                    <Button
                      type="button"
                      variant="outline"
                      className="w-full justify-start gap-2 min-h-11"
                      disabled={!wallet.installed || isConnecting}
                      onClick={() => {
                        void live.connect(wallet.id).then(() => setPickerOpen(false));
                      }}
                    >
                      <Plug className="h-4 w-4" aria-hidden />
                      {wallet.label}
                      {!wallet.installed && (
                        <span className="text-muted-foreground"> (not installed)</span>
                      )}
                    </Button>
                  </li>
                ))}
              </ul>
            )}
            {isConnected && (
              <Button
                type="button"
                variant="ghost"
                className="min-h-11 gap-2"
                onClick={() => void live.disconnect()}
              >
                <Unplug className="h-4 w-4" aria-hidden />
                Disconnect
              </Button>
            )}
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}

function StellarWalletChip({
  className,
  storyState,
}: {
  className?: string;
  storyState?: CrossChainWalletStoryState;
}) {
  const wallet = useWallet();

  const isConnecting = storyState === 'connecting' || wallet.isLoading;
  const isConnected =
    storyState === 'connected' ||
    (storyState === undefined && wallet.isConnected);
  const networkMismatch =
    storyState === 'mismatch' ||
    (storyState === undefined && wallet.networkMismatch);

  const statusLabel = networkMismatch
    ? 'Network mismatch'
    : isConnecting
      ? 'Connecting'
      : isConnected
        ? 'Connected'
        : 'Disconnected';

  return (
    <button
      type="button"
      onClick={() => {
        if (!isConnected) void wallet.connect('freighter');
      }}
      className={cn(
        'flex min-h-11 items-center gap-2 rounded-xl border px-3 py-2 text-left',
        networkMismatch
          ? 'border-signal/40 bg-signal/10'
          : isConnected
            ? 'border-primary/35 bg-primary/10'
            : 'border-border/50 bg-background/40',
        className
      )}
      aria-label={`Stellar wallet: ${statusLabel}`}
      data-testid="wallet-chip-stellar"
    >
      <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-muted/50">
        {isConnecting ? (
          <Loader2 className="h-4 w-4 animate-spin" aria-hidden />
        ) : (
          <Wallet className="h-4 w-4" aria-hidden />
        )}
      </span>
      <span className="min-w-0">
        <span className="block text-[10px] uppercase tracking-wide text-muted-foreground">
          {statusLabel}
        </span>
        <span className="block truncate font-mono text-xs font-semibold">
          {isConnected && wallet.address
            ? shortenAddress(wallet.address)
            : 'Stellar'}
        </span>
      </span>
    </button>
  );
}
