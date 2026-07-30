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

function renderDeck(
  presentation?: Parameters<typeof CrossChainSwapDeck>[0]['storyPresentation']
) {
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

  it('blocks uncatalogued Sepolia → Bitcoin with unsupported badge and no CTA', async () => {
    const user = userEvent.setup();
    renderDeck();

    await user.click(screen.getByTestId('chain-option-source-ethereum-sepolia'));
    await user.click(screen.getByTestId('chain-option-destination-bitcoin'));

    expect(screen.getByTestId('unsupported-corridor-alert')).toBeInTheDocument();
    expect(screen.getByTestId('corridor-status-badge')).toHaveTextContent(
      /unsupported pair/i
    );
    expect(screen.queryByTestId('cross-chain-review-cta')).not.toBeInTheDocument();
    expect(screen.queryByText(/99\./)).not.toBeInTheDocument();
  });

  it('renders CCTP preview rail without destination amount', () => {
    renderDeck({
      initialSourceChainId: 'ethereum-sepolia',
      initialDestChainId: 'stellar',
    });
    expect(screen.getByLabelText('CCTP protocol steps')).toBeInTheDocument();
    expect(screen.getAllByText(/preview — not live/i).length).toBeGreaterThan(0);
  });

  it('keeps cross-chain review CTA disabled without handler', () => {
    renderDeck({
      initialSourceChainId: 'ethereum-sepolia',
      initialDestChainId: 'stellar',
    });
    const cta = screen.getByTestId('cross-chain-review-cta');
    expect(cta).toBeDisabled();
  });

  it('exposes timeline list semantics with aria-current on active step', () => {
    renderDeck({ timelineSteps: EXECUTING_TIMELINE_STORY_FIXTURE });
    const timeline = screen.getByTestId('execution-timeline');
    expect(timeline.querySelector('[aria-current="step"]')).toBeTruthy();
    expect(screen.getByText(/Support ref: SR-FIXTURE-002/)).toBeInTheDocument();
  });
});

describe('CrossChainSwapDeck recipient validation', () => {
  it('blocks review when override address is invalid', async () => {
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
    expect(screen.getByTestId('cross-chain-review-cta')).toBeDisabled();
  });
});
