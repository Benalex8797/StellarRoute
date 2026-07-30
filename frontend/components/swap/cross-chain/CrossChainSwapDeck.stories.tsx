import type { Story } from '@ladle/react';
import '@/app/globals.css';
import { CrossChainSwapDeck } from './CrossChainSwapDeck';
import { SettingsProvider } from '@/components/providers/settings-provider';
import { WalletProvider } from '@/components/providers/wallet-provider';
import { ThemeProvider } from 'next-themes';
import type { CrossChainSwapStoryFixture } from '@/hooks/useCrossChainSwapState';

function DeckStory({ fixture }: { fixture: CrossChainSwapStoryFixture }) {
  return (
    <ThemeProvider attribute="class" defaultTheme="dark" enableSystem>
      <SettingsProvider>
        <WalletProvider>
          <div className="dark min-h-screen bg-background p-6 text-foreground">
            <CrossChainSwapDeck storyFixture={fixture} />
          </div>
        </WalletProvider>
      </SettingsProvider>
    </ThemeProvider>
  );
}

export const StellarNative: Story = () => <DeckStory fixture="stellar-native" />;
StellarNative.storyName = 'Stellar native — delegates SwapCard';

export const EvmToStellarComingSoon: Story = () => (
  <DeckStory fixture="evm-to-stellar" />
);
EvmToStellarComingSoon.storyName = 'EVM → Stellar — coming soon';

export const WalletsPartial: Story = () => <DeckStory fixture="wallets-partial" />;
WalletsPartial.storyName = 'Wallets — partial connect';

export const NetworkMismatch: Story = () => <DeckStory fixture="network-mismatch" />;
NetworkMismatch.storyName = 'Network mismatch';

export const RoutePreview: Story = () => <DeckStory fixture="evm-to-stellar" />;
RoutePreview.storyName = 'Route preview — CCTP rail';

export const ExecutingTimeline: Story = () => <DeckStory fixture="executing-timeline" />;
ExecutingTimeline.storyName = 'Executing timeline fixture';

export const UnsupportedCorridor: Story = () => (
  <DeckStory fixture="unsupported-corridor" />
);
UnsupportedCorridor.storyName = 'Unsupported corridor alert';
