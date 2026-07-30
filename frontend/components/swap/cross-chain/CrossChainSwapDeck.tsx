'use client';

import dynamic from 'next/dynamic';
import { useEffect, useMemo } from 'react';
import { NetworkMismatchBanner } from '@/components/shared/NetworkMismatchBanner';
import { useFeatureFlag } from '@/hooks/useFeatureFlag';
import { useCrossChainSwapState } from '@/hooks/useCrossChainSwapState';
import { useApiV2Readiness } from '@/hooks/useApiV2Readiness';
import { useCctpSaga } from '@/hooks/useCctpSaga';
import { useChainWallet } from '@/hooks/useChainWallet';
import { useWallet } from '@/components/providers/wallet-provider';
import { UNMATCHED_CORRIDOR_ID } from '@/lib/cross-chain/corridors';
import { corridorStatusCopy } from '@/lib/cross-chain/format';
import { resolveCctpDirection } from '@/lib/cctp/corridor-bridge';
import { cn } from '@/lib/utils';
import { CctpExecutionPanel } from './CctpExecutionPanel';
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
  const readiness = useApiV2Readiness({ refreshMs: 60_000 });
  const stellarWallet = useWallet();
  const sourceEvm = useChainWallet({
    chainFamily: 'evm',
    expectedNetwork: 'eip155:11155111',
  });
  const destStellarMint = useChainWallet({
    chainFamily: 'stellar',
    expectedNetwork: 'stellar:testnet',
  });

  const cctpDirection = resolveCctpDirection(state.sourceChainId, state.destChainId);
  const recipient =
    state.useRecipientOverride && state.recipientOverride.trim()
      ? state.recipientOverride.trim()
      : state.destChain.chainFamily === 'stellar'
        ? stellarWallet.address ?? ''
        : sourceEvm.session?.account.address ?? '';

  const quoteInputsKey = useMemo(
    () =>
      [
        state.sourceChainId,
        state.destChainId,
        state.sourceAmount,
        recipient,
        stellarWallet.address ?? '',
        sourceEvm.session?.account.address ?? '',
      ].join('|'),
    [
      state.sourceChainId,
      state.destChainId,
      state.sourceAmount,
      recipient,
      stellarWallet.address,
      sourceEvm.session?.account.address,
    ],
  );

  const saga = useCctpSaga({
    sourceChainId: state.sourceChainId,
    destChainId: state.destChainId,
    amount: state.sourceAmount || '0',
    recipient,
    sender:
      state.sourceChain.chainFamily === 'stellar'
        ? stellarWallet.address ?? undefined
        : sourceEvm.session?.account.address,
    mintSubmitter:
      cctpDirection === 'evm_to_stellar'
        ? destStellarMint.session?.account.address ?? stellarWallet.address ?? undefined
        : undefined,
    wallets: {
      sourceStellarAdapterId:
        state.sourceChain.chainFamily === 'stellar'
          ? stellarWallet.walletId ?? undefined
          : undefined,
      sourceEvmAdapterId:
        state.sourceChain.chainFamily === 'evm'
          ? sourceEvm.session?.adapterId
          : undefined,
      mintSubmitterStellarAdapterId:
        destStellarMint.session?.adapterId ?? stellarWallet.walletId ?? undefined,
      recipient,
      mintSubmitter:
        destStellarMint.session?.account.address ?? stellarWallet.address ?? undefined,
    },
    bridgeReady: state.executable && Boolean(cctpDirection) && readiness.cctpGloballyReady,
    quoteInputsKey,
  });

  useEffect(() => {
    if (state.executable && cctpDirection) {
      void saga.reconcileOnLoad();
    }
  }, [state.executable, cctpDirection, saga.reconcileOnLoad]);

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
                  {state.executable && cctpDirection && (
                    <label className="block space-y-1">
                      <span className="text-xs font-medium text-muted-foreground">
                        USDC amount (source)
                      </span>
                      <input
                        type="text"
                        inputMode="decimal"
                        className="min-h-11 w-full rounded-xl border border-border/50 bg-background/60 px-3 font-mono text-sm"
                        value={state.sourceAmount}
                        onChange={(e) => state.setSourceAmount(e.target.value)}
                        placeholder="0.00"
                        data-testid="cctp-source-amount"
                      />
                    </label>
                  )}
                  {state.executable && cctpDirection && (
                    <CctpExecutionPanel
                      stage={saga.stage}
                      quote={saga.quote}
                      transferStatus={saga.transferStatus}
                      error={saga.error}
                      primaryLabel={saga.primaryAction.label}
                      primaryDisabled={
                        saga.primaryAction.disabled ||
                        !state.sourceAmount ||
                        !recipient ||
                        readiness.loading
                      }
                      onPrimary={() => void saga.runPrimaryAction()}
                      onReset={saga.resetSaga}
                      bridgeUnavailable={
                        readiness.loaded && !readiness.cctpGloballyReady
                      }
                    />
                  )}
                </>
              )}
            </div>
          )}
        </div>

        <aside className="space-y-4 min-w-0" aria-label="Route and execution details">
          <CrossChainRoutePanel
            sourceChainId={state.sourceChainId}
            destChainId={state.destChainId}
            protocol={state.corridor?.protocol ?? null}
            executable={state.executable && readiness.cctpGloballyReady}
            uncatalogued={state.isUncatalogued}
            quote={saga.quote}
            bridgeUnavailable={readiness.loaded && !readiness.cctpGloballyReady}
            sagaStatus={saga.transferStatus?.status}
          />
          <RouteDisclosurePanel />
          <CrossChainExecutionTimeline steps={state.timelineSteps} />
        </aside>
      </div>
    </div>
  );
}
