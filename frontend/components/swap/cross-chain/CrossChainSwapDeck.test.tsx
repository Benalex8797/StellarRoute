import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { CrossChainSwapDeck } from './CrossChainSwapDeck';
import { SettingsProvider } from '@/components/providers/settings-provider';
import { WalletProvider } from '@/components/providers/wallet-provider';
import { EXECUTING_TIMELINE_STORY_FIXTURE } from './crossChainStoryPresentation';

vi.mock('next/dynamic', () => ({
  default: () => {
    const MockSwapCard = () => (
      <div data-testid="swap-card">Delegated SwapCard</div>
    );
    return MockSwapCard;
  },
}));

vi.mock('@/hooks/useFeatureFlag', () => ({
  useFeatureFlag: vi.fn(() => ({ enabled: false, loading: false })),
}));

vi.mock('@/hooks/useApiV2Readiness', () => ({
  useApiV2Readiness: vi.fn(() => ({
    loaded: true,
    corridors: [],
    cctpGloballyReady: false,
    providerKilled: false,
    error: null,
    fetchedAt: Date.now(),
    loading: false,
    refresh: vi.fn(),
  })),
}));

vi.mock('@/hooks/useCrossChainWalletRoles', () => ({
  useCrossChainWalletRoles: vi.fn(() => ({
    direction: null,
    destRecipientAddress: '',
    isMuxedRecipient: false,
    showMintSubmitterChip: false,
    sourceChipBinding: null,
    destChipBinding: null,
    mintSubmitterChipBinding: null,
    sagaWallets: { recipient: '' },
  })),
}));

vi.mock('@/hooks/useCctpSaga', () => ({
  useCctpSaga: vi.fn(() => ({
    stage: 'idle',
    quote: null,
    transferStatus: null,
    error: null,
    busy: false,
    inputsLocked: false,
    resumeMismatch: false,
    sessionPublic: null,
    primaryAction: { label: 'Get CCTP quote', disabled: false, action: 'quote' },
    runPrimaryAction: vi.fn(),
    requestQuote: vi.fn(),
    reconcileOnLoad: vi.fn(),
    resetSaga: vi.fn(),
    reattestCooldownUntil: null,
  })),
}));

vi.mock('@/hooks/useChainWallet', () => ({
  useChainWallet: vi.fn(() => ({
    session: null,
    isConnected: false,
    networkMismatch: false,
    isLoading: false,
    availableWallets: [],
    connect: vi.fn(),
    disconnect: vi.fn(),
  })),
}));

import type { CrossChainDeckStoryPresentation } from './crossChainStoryPresentation';

function renderDeck(presentation?: CrossChainDeckStoryPresentation) {
  return render(
    <SettingsProvider>
      <WalletProvider>
        <CrossChainSwapDeck storyPresentation={presentation} />
      </WalletProvider>
    </SettingsProvider>
  );
}

describe('CrossChainSwapDeck', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows paired chain selectors and delegates SwapCard on stellar-native', () => {
    renderDeck();
    expect(screen.getByTestId('paired-chain-selectors')).toBeInTheDocument();
    expect(screen.getByTestId('chain-leg-source')).toBeInTheDocument();
    expect(screen.getByTestId('chain-leg-destination')).toBeInTheDocument();
    expect(screen.getByTestId('stellar-native-delegation')).toBeInTheDocument();
    expect(screen.getByTestId('swap-card')).toBeInTheDocument();
    expect(screen.getByTestId('corridor-status-badge')).toHaveTextContent(
      /executable corridor/i
    );
  });

  it('shows unsupported alert for catalogued coming-soon corridor', () => {
    renderDeck({
      initialSourceChainId: 'ethereum-sepolia',
      initialDestChainId: 'stellar',
    });
    expect(screen.getByTestId('unsupported-corridor-alert')).toBeInTheDocument();
    expect(screen.queryByTestId('swap-card')).not.toBeInTheDocument();
    expect(screen.getByTestId('corridor-status-badge')).toHaveTextContent(
      /coming soon/i
    );
  });

  it('blocks uncatalogued Sepolia to Bitcoin with unsupported badge and no CTA', async () => {
    const user = userEvent.setup();
    renderDeck();

    await user.click(screen.getByTestId('chain-option-source-ethereum-sepolia'));
    await user.click(screen.getByTestId('chain-option-destination-bitcoin'));

    expect(screen.getByTestId('unsupported-corridor-alert')).toBeInTheDocument();
    expect(screen.getByTestId('corridor-status-badge')).toHaveTextContent(
      /unsupported pair/i
    );
    expect(screen.queryByTestId('cross-chain-review-cta')).not.toBeInTheDocument();
    expect(screen.queryByTestId('cctp-route-rail')).not.toBeInTheDocument();
    expect(screen.queryByText(/99\./)).not.toBeInTheDocument();
  });

  it('renders CCTP preview rail for catalogued cross-chain corridor only', () => {
    renderDeck({
      initialSourceChainId: 'ethereum-sepolia',
      initialDestChainId: 'stellar',
    });
    expect(screen.getByTestId('cctp-route-rail')).toBeInTheDocument();
    expect(screen.getAllByText(/preview — not live/i).length).toBeGreaterThan(0);
  });

  it('does not render cross-chain review CTA without backend handler', () => {
    renderDeck({
      initialSourceChainId: 'ethereum-sepolia',
      initialDestChainId: 'stellar',
    });
    expect(screen.queryByTestId('cross-chain-review-cta')).not.toBeInTheDocument();
  });

  it('exposes timeline list semantics with aria-current on active step', () => {
    renderDeck({ timelineSteps: EXECUTING_TIMELINE_STORY_FIXTURE });
    const timeline = screen.getByTestId('execution-timeline');
    expect(timeline.querySelector('[aria-current="step"]')).toBeTruthy();
    expect(screen.getByText(/Support ref: SR-FIXTURE-002/)).toBeInTheDocument();
  });
});

describe('CrossChainSwapDeck recipient validation', () => {
  it('shows validation error for invalid recipient override', async () => {
    const user = userEvent.setup();
    renderDeck({
      initialSourceChainId: 'ethereum-sepolia',
      initialDestChainId: 'stellar',
    });
    await user.click(screen.getByLabelText('Use custom destination recipient'));
    const input = screen.getByTestId('destination-recipient-input');
    await user.type(input, 'not-valid');
    const alerts = screen.getAllByRole('alert');
    expect(
      alerts.some((el) => /Stellar account/i.test(el.textContent ?? ''))
    ).toBe(true);
  });
});
