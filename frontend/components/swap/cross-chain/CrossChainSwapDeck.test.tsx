import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { CrossChainSwapDeck } from './CrossChainSwapDeck';
import { SettingsProvider } from '@/components/providers/settings-provider';
import { WalletProvider } from '@/components/providers/wallet-provider';
import { EXECUTING_TIMELINE_STORY_FIXTURE } from './crossChainStoryPresentation';
import { useApiV2Readiness } from '@/hooks/useApiV2Readiness';
import { useCrossChainWalletRoles } from '@/hooks/useCrossChainWalletRoles';
import type { UseCrossChainWalletRolesInput } from '@/hooks/useCrossChainWalletRoles';

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
  useCrossChainWalletRoles: vi.fn(
    (input: UseCrossChainWalletRolesInput) => ({
      direction: null,
      destRecipientAddress: '',
      isMuxedRecipient: false,
      showMintSubmitterChip: false,
      sourceChipBinding: null,
      destChipBinding: null,
      mintSubmitterChipBinding: null,
      sagaWallets: { recipient: '' },
      _input: input,
    }),
  ),
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

const mockUseApiV2Readiness = vi.mocked(useApiV2Readiness);
const mockUseCrossChainWalletRoles = vi.mocked(useCrossChainWalletRoles);

function mockStellarToSepoliaWalletRoles(
  input: UseCrossChainWalletRolesInput,
) {
  const destRecipientAddress =
    input.useRecipientOverride && input.recipientOverride?.trim()
      ? input.recipientOverride.trim()
      : '';
  return {
    direction: 'stellar_to_evm' as const,
    destRecipientAddress,
    isMuxedRecipient: false,
    showMintSubmitterChip: false,
    sourceChipBinding: null,
    destChipBinding: null,
    mintSubmitterChipBinding: null,
    sagaWallets: { recipient: destRecipientAddress },
  };
}

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
    renderDeck({
      initialSourceChainId: 'stellar',
      initialDestChainId: 'stellar',
    });
    expect(screen.getByTestId('paired-chain-selectors')).toBeInTheDocument();
    expect(screen.getByTestId('chain-leg-source')).toBeInTheDocument();
    expect(screen.getByTestId('chain-leg-destination')).toBeInTheDocument();
    expect(screen.getByTestId('stellar-native-delegation')).toBeInTheDocument();
    expect(screen.getByTestId('swap-card')).toBeInTheDocument();
    expect(screen.getByTestId('corridor-status-badge')).toHaveTextContent(
      /executable corridor/i
    );
  });

  it('defaults to the proven Stellar to Sepolia corridor', () => {
    renderDeck();

    expect(screen.getByTestId('chain-option-source-stellar')).toBeChecked();
    expect(
      screen.getByTestId('chain-option-destination-ethereum-sepolia')
    ).toBeChecked();
    expect(screen.queryByTestId('swap-card')).not.toBeInTheDocument();
    expect(screen.queryByTestId('unsupported-corridor-alert')).not.toBeInTheDocument();
    expect(screen.getByTestId('cctp-route-rail')).toBeInTheDocument();
    expect(screen.getByTestId('corridor-status-badge')).toHaveTextContent(
      /executable corridor/i
    );
    // Settlement still waits on API readiness in this mock.
    expect(
      screen.getByText(/CCTP corridor is listed but not executable on this API yet/i)
    ).toBeInTheDocument();
  });

  it('treats Sepolia to Stellar as a catalog-executable CCTP corridor', () => {
    renderDeck({
      initialSourceChainId: 'ethereum-sepolia',
      initialDestChainId: 'stellar',
    });
    expect(screen.queryByTestId('unsupported-corridor-alert')).not.toBeInTheDocument();
    expect(screen.queryByTestId('swap-card')).not.toBeInTheDocument();
    expect(screen.getByTestId('cctp-route-rail')).toBeInTheDocument();
    expect(screen.getByTestId('corridor-status-badge')).toHaveTextContent(
      /executable corridor/i
    );
  });

  it('shows unsupported alert for catalogued coming-soon corridor', () => {
    renderDeck({
      initialSourceChainId: 'solana',
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

describe('CrossChainSwapDeck CCTP CTA hints', () => {
  beforeEach(() => {
    mockUseApiV2Readiness.mockReturnValue({
      loaded: true,
      corridors: [],
      cctpGloballyReady: true,
      providerKilled: false,
      error: null,
      fetchedAt: Date.now(),
      loading: false,
      refresh: vi.fn(),
    });
    mockUseCrossChainWalletRoles.mockImplementation(mockStellarToSepoliaWalletRoles);
  });

  it('shows missing-destination hint when ETH disconnected and override is off', () => {
    renderDeck();

    expect(screen.getByTestId('cross-chain-review-cta')).toBeDisabled();
    expect(screen.getByTestId('cctp-cta-hint')).toHaveTextContent(
      /Connect ETH Sepolia or enable Destination recipient/i,
    );
    expect(screen.getByTestId('dest-wallet-setup-hint')).toHaveTextContent(
      /Connect ETH Sepolia/i,
    );
    expect(screen.getByTestId('destination-recipient-setup-hint')).toHaveTextContent(
      /paste a 0x address/i,
    );
  });

  it('clears destination hint and enables CTA when override provides valid 0x', async () => {
    const user = userEvent.setup();
    renderDeck();

    await user.click(screen.getByLabelText('Use custom destination recipient'));
    const recipientInput = screen.getByTestId('destination-recipient-input');
    await user.type(
      recipientInput,
      '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0',
    );
    await user.type(screen.getByTestId('cctp-source-amount'), '10');

    expect(screen.queryByTestId('cctp-cta-hint')).not.toBeInTheDocument();
    expect(screen.queryByTestId('dest-wallet-setup-hint')).not.toBeInTheDocument();
    expect(
      screen.queryByTestId('destination-recipient-setup-hint'),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId('cross-chain-review-cta')).toBeEnabled();
  });

  it('surfaces USDC-only guidance with swap link on CCTP corridor', () => {
    renderDeck();

    expect(screen.getByTestId('cctp-usdc-only-note')).toHaveTextContent(
      /CCTP bridges native USDC only/i,
    );
    expect(screen.getByTestId('swap-to-usdc-on-stellar-link')).toBeInTheDocument();
  });
});
