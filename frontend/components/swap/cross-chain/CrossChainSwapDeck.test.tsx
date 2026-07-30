import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { CrossChainSwapDeck } from './CrossChainSwapDeck';
import { SettingsProvider } from '@/components/providers/settings-provider';
import { WalletProvider } from '@/components/providers/wallet-provider';

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

function renderDeck(fixture?: Parameters<typeof CrossChainSwapDeck>[0]['storyFixture']) {
  return render(
    <SettingsProvider>
      <WalletProvider>
        <CrossChainSwapDeck storyFixture={fixture} />
      </WalletProvider>
    </SettingsProvider>
  );
}

describe('CrossChainSwapDeck', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('delegates stellar-native corridor to SwapCard', () => {
    renderDeck('stellar-native');
    expect(screen.getByTestId('stellar-native-delegation')).toBeInTheDocument();
    expect(screen.getByTestId('swap-card')).toBeInTheDocument();
  });

  it('shows unsupported alert for non-executable corridor', () => {
    renderDeck('evm-to-stellar');
    expect(screen.getByTestId('unsupported-corridor-alert')).toBeInTheDocument();
    expect(screen.queryByTestId('swap-card')).not.toBeInTheDocument();
  });

  it('disables review CTA for coming-soon corridor', () => {
    renderDeck('evm-to-stellar');
    expect(screen.getByTestId('cross-chain-review-cta')).toBeDisabled();
  });

  it('renders CCTP preview rail without destination amount', () => {
    renderDeck('evm-to-stellar');
    expect(screen.getByLabelText('CCTP protocol steps')).toBeInTheDocument();
    expect(screen.getAllByText(/preview — not live/i).length).toBeGreaterThan(0);
    expect(screen.queryByText(/99\./)).not.toBeInTheDocument();
  });

  it('exposes timeline list semantics with aria-current on active step', () => {
    renderDeck('executing-timeline');
    const timeline = screen.getByTestId('execution-timeline');
    expect(timeline.querySelector('[aria-current="step"]')).toBeTruthy();
    expect(screen.getByText(/Support ref: SR-FIXTURE-002/)).toBeInTheDocument();
  });
});

describe('CrossChainSwapDeck recipient validation', () => {
  it('blocks review when override address is invalid', async () => {
    const user = userEvent.setup();
    renderDeck('evm-to-stellar');
    await user.click(screen.getByLabelText('Use custom destination recipient'));
    const input = screen.getByTestId('destination-recipient-input');
    await user.type(input, 'not-valid');
    const alerts = screen.getAllByRole('alert');
    expect(
      alerts.some((el) => /valid Stellar address/i.test(el.textContent ?? ''))
    ).toBe(true);
    expect(screen.getByTestId('cross-chain-review-cta')).toBeDisabled();
  });
});
