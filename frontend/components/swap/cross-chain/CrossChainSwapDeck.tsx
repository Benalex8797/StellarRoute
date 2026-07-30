'use client';

import dynamic from 'next/dynamic';
import { Button } from '@/components/ui/button';
import { NetworkMismatchBanner } from '@/components/shared/NetworkMismatchBanner';
import { useFeatureFlag } from '@/hooks/useFeatureFlag';
import {
  useCrossChainSwapState,
  type CrossChainSwapStoryFixture,
} from '@/hooks/useCrossChainSwapState';
import { corridorStatusCopy } from '@/lib/cross-chain/format';
import { cn } from '@/lib/utils';
import { ChainAssetLeg } from './ChainAssetLeg';
import { CorridorTabs } from './CorridorTabs';
import { CrossChainExecutionTimeline } from './CrossChainExecutionTimeline';
import { CrossChainRoutePanel } from './CrossChainRoutePanel';
import { DestinationAddressField } from './DestinationAddressField';
import { RouteDisclosurePanel } from './RouteDisclosurePanel';
import { UnsupportedCorridorState } from './UnsupportedCorridorState';

const SwapCard = dynamic(
  () => import('@/components/swap/SwapCard').then((m) => m.SwapCard),
  {
    ssr: false,
    loading: () => (
      <div
        className="flex h-[480px] items-center justify-center rounded-2xl chart-panel"
        data-testid="swap-card-loading"
      >
        <div className="h-8 w-8 rounded-full border-4 border-primary border-t-transparent animate-spin" />
      </div>
    ),
  }
);

export interface CrossChainSwapDeckProps {
  storyFixture?: CrossChainSwapStoryFixture;
}

function walletStoryForFixture(
  fixture: CrossChainSwapStoryFixture,
  leg: 'source' | 'dest'
): 'disconnected' | 'connecting' | 'connected' | 'mismatch' | 'unsupported' | undefined {
  if (fixture === 'wallets-partial') {
    return leg === 'source' ? 'connected' : 'disconnected';
  }
  if (fixture === 'network-mismatch') {
    return 'mismatch';
  }
  return undefined;
}

export function CrossChainSwapDeck({ storyFixture }: CrossChainSwapDeckProps = {}) {
  const state = useCrossChainSwapState({ storyFixture });
  const { enabled: routesBeta } = useFeatureFlag('routes_beta');

  const sourceWalletStory = walletStoryForFixture(state.fixture, 'source');
  const destWalletStory = walletStoryForFixture(state.fixture, 'dest');

  return (
    <div
      className="cross-chain-deck w-full mx-auto space-y-5"
      data-testid="cross-chain-swap-deck"
    >
      <header className="space-y-2 px-1 sm:px-0">
        <p className="font-mono text-[11px] uppercase tracking-[0.18em] text-primary">
          Cross-chain route
        </p>
        <div className="flex flex-wrap items-end justify-between gap-3">
          <h2 className="brand-wordmark text-2xl text-foreground sm:text-3xl">
            Stellar-centered routing
          </h2>
          <span
            className={cn(
              'rounded-full border px-3 py-1 font-mono text-[10px] uppercase tracking-wider',
              state.executable
                ? 'border-primary/40 bg-primary/10 text-primary'
                : 'border-border/50 text-muted-foreground'
            )}
          >
            {corridorStatusCopy(state.executable)}
          </span>
        </div>
        <p className="max-w-2xl text-sm text-muted-foreground">
          Source and destination chains stay visible. Only executable corridors
          reach review — previews explain protocol steps without fake quotes.
        </p>
      </header>

      <CorridorTabs activeId={state.corridorId} onSelect={state.selectCorridor} />

      <div
        className="cross-chain-deck-grid gap-5 lg:gap-6"
        id={`corridor-panel-${state.corridorId}`}
        role="tabpanel"
        aria-labelledby={`corridor-tab-${state.corridorId}`}
      >
        <div className="space-y-4 min-w-0">
          {state.isStellarNative ? (
            <div className="space-y-4" data-testid="stellar-native-delegation">
              <NetworkMismatchBanner />
              <SwapCard showRoutePicker={routesBeta} />
            </div>
          ) : (
            <div className="space-y-4">
              <ChainAssetLeg
                role="source"
                chain={state.sourceChain}
                chainId={state.sourceChainId}
                onChainChange={state.selectSourceChain}
                amount={state.sourceAmount}
                onAmountChange={state.setSourceAmount}
                amountDisabled={!state.executable}
                walletStoryState={sourceWalletStory}
              />
              <ChainAssetLeg
                role="destination"
                chain={state.destChain}
                chainId={state.destChainId}
                onChainChange={state.selectDestChain}
                amountReadOnly
                amountDisabled
                walletStoryState={destWalletStory}
              />
              <UnsupportedCorridorState
                sourceChainId={state.sourceChainId}
                destChainId={state.destChainId}
              />
              <DestinationAddressField
                chain={state.destChain}
                enabled={state.useRecipientOverride}
                onEnabledChange={state.setUseRecipientOverride}
                value={state.recipientOverride}
                onChange={state.setRecipientOverride}
                validation={state.recipientValidation}
              />
              <Button
                type="button"
                className="w-full min-h-11"
                disabled={!state.canReview}
                data-testid="cross-chain-review-cta"
              >
                Review cross-chain route
              </Button>
            </div>
          )}
        </div>

        <aside className="space-y-4 min-w-0" aria-label="Route and execution details">
          <CrossChainRoutePanel
            sourceChainId={state.sourceChainId}
            destChainId={state.destChainId}
            protocol={state.corridor.protocol}
            executable={state.executable}
          />
          <RouteDisclosurePanel />
          <CrossChainExecutionTimeline steps={state.timelineSteps} />
        </aside>
      </div>
    </div>
  );
}

export function CrossChainSwapDeckSkeleton() {
  return (
    <div
      className="cross-chain-deck w-full mx-auto space-y-5"
      data-testid="cross-chain-swap-deck-skeleton"
      aria-busy="true"
      aria-label="Loading cross-chain swap interface"
    >
      <div className="space-y-2">
        <div className="h-3 w-32 rounded bg-muted animate-pulse" />
        <div className="h-8 w-64 max-w-full rounded bg-muted animate-pulse" />
        <div className="h-4 w-full max-w-xl rounded bg-muted animate-pulse" />
      </div>
      <div className="flex flex-wrap gap-2">
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="h-11 w-28 rounded-xl bg-muted animate-pulse" />
        ))}
      </div>
      <div className="cross-chain-deck-grid gap-5">
        <div className="h-[520px] rounded-2xl chart-panel animate-pulse bg-muted/30" />
        <div className="h-[320px] rounded-2xl chart-panel animate-pulse bg-muted/30" />
      </div>
    </div>
  );
}
