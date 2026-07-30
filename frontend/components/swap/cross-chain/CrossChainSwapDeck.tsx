'use client';

import dynamic from 'next/dynamic';
import { Button } from '@/components/ui/button';
import { NetworkMismatchBanner } from '@/components/shared/NetworkMismatchBanner';
import { useFeatureFlag } from '@/hooks/useFeatureFlag';
import { useCrossChainSwapState } from '@/hooks/useCrossChainSwapState';
import { UNMATCHED_CORRIDOR_ID } from '@/lib/cross-chain/corridors';
import { corridorStatusCopy } from '@/lib/cross-chain/format';
import { cn } from '@/lib/utils';
import { CorridorTabs } from './CorridorTabs';
import { CrossChainExecutionTimeline } from './CrossChainExecutionTimeline';
import { CrossChainRoutePanel } from './CrossChainRoutePanel';
import { DestinationAddressField } from './DestinationAddressField';
import { PairedChainSelectors } from './PairedChainSelectors';
import { RouteDisclosurePanel } from './RouteDisclosurePanel';
import { UnsupportedCorridorState } from './UnsupportedCorridorState';
import type { CrossChainDeckStoryPresentation } from './crossChainStoryPresentation';

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
  storyPresentation?: CrossChainDeckStoryPresentation;
}

export function CrossChainSwapDeck({
  storyPresentation,
}: CrossChainSwapDeckProps = {}) {
  const state = useCrossChainSwapState({
    timelineStepsOverride: storyPresentation?.timelineSteps,
    initialSourceChainId: storyPresentation?.initialSourceChainId,
    initialDestChainId: storyPresentation?.initialDestChainId,
  });
  const { enabled: routesBeta } = useFeatureFlag('routes_beta');

  const panelId =
    state.corridorId === UNMATCHED_CORRIDOR_ID
      ? 'corridor-panel-unmatched'
      : `corridor-panel-${state.corridorId}`;
  const panelLabelId =
    state.corridorId === UNMATCHED_CORRIDOR_ID
      ? 'corridor-tab-unmatched'
      : `corridor-tab-${state.corridorId}`;

  const showCrossChainPreview =
    !state.isStellarNativeExecutable && !state.isUncatalogued;
  const showUnsupported =
    state.isUncatalogued || (!state.executable && !state.isStellarNativeExecutable);

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
              state.executable && !state.isUncatalogued
                ? 'border-primary/40 bg-primary/10 text-primary'
                : 'border-border/50 text-muted-foreground'
            )}
            data-testid="corridor-status-badge"
          >
            {corridorStatusCopy(state.executable, state.isUncatalogued)}
          </span>
        </div>
        <p className="max-w-2xl text-sm text-muted-foreground">
          Source and destination chains stay visible. Only executable corridors
          reach review — previews explain protocol steps without fake quotes.
        </p>
      </header>

      <CorridorTabs activeId={state.corridorId} onSelect={state.selectCorridor} />

      <PairedChainSelectors
        sourceChainId={state.sourceChainId}
        destChainId={state.destChainId}
        onSourceChange={state.selectSourceChain}
        onDestChange={state.selectDestChain}
        sourceWalletState={storyPresentation?.sourceWalletState}
        destWalletState={storyPresentation?.destWalletState}
      />

      <div
        className="cross-chain-deck-grid gap-5 lg:gap-6"
        id={panelId}
        role="tabpanel"
        aria-labelledby={panelLabelId}
      >
        <div className="space-y-4 min-w-0">
          {state.isStellarNativeExecutable ? (
            <div className="space-y-3" data-testid="stellar-native-delegation">
              <p className="text-xs text-muted-foreground">
                Amounts, assets, and quotes are edited in the Stellar swap card
                below — your single source for live execution.
              </p>
              <NetworkMismatchBanner />
              <SwapCard showRoutePicker={routesBeta} />
            </div>
          ) : (
            <div className="space-y-4">
              {showUnsupported && (
                <UnsupportedCorridorState
                  sourceChainId={state.sourceChainId}
                  destChainId={state.destChainId}
                  uncatalogued={state.isUncatalogued}
                />
              )}
              {showCrossChainPreview && (
                <>
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
                    disabled
                    data-testid="cross-chain-review-cta"
                    title="Cross-chain execution is not available until backend routes exist"
                  >
                    Review cross-chain route
                  </Button>
                </>
              )}
            </div>
          )}
        </div>

        <aside className="space-y-4 min-w-0" aria-label="Route and execution details">
          <CrossChainRoutePanel
            sourceChainId={state.sourceChainId}
            destChainId={state.destChainId}
            protocol={state.corridor?.protocol ?? 'cctp-preview'}
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
