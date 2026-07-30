/**
 * CCTP cross-chain swap E2E — mocked API + fake wallets (no real network).
 */
import { test, expect, type Page } from '@playwright/test';

const EVM_ADDR = '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0';
const STELLAR_G = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';

function jsonData(payload: unknown) {
  return JSON.stringify({ data: payload });
}

function installFakeWallets(page: Page) {
  return page.addInitScript(() => {
    (window as unknown as { __STELLAR_ROUTE_FLAGS__?: Record<string, boolean> }).__STELLAR_ROUTE_FLAGS__ = {
      swap_ui_v2: true,
    };
    localStorage.setItem('stellarroute:onboarding:dismissed', 'true');
    localStorage.setItem('stellarroute.onboarding.seen', 'true');
    localStorage.setItem('stellarroute.onboarding.completed', 'true');

    let sendCount = 0;
    (window as unknown as { __cctpWalletSendCount?: () => number }).__cctpWalletSendCount = () =>
      sendCount;

    const ethereum = {
      isMetaMask: true,
      request: async ({ method, params }: { method: string; params?: unknown[] }) => {
        if (method === 'eth_requestAccounts' || method === 'eth_accounts') {
          return ['0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0'];
        }
        if (method === 'eth_chainId') return '0xaa36a7';
        if (method === 'wallet_switchEthereumChain') return null;
        if (method === 'eth_sendTransaction') {
          sendCount += 1;
          return '0xdeadbeef';
        }
        if (method === 'personal_sign') return '0xsig';
        return null;
      },
    };
    Object.defineProperty(window, 'ethereum', { value: ethereum, configurable: true });

    (window as unknown as { freighter?: unknown }).freighter = {
      isConnected: async () => true,
      isAllowed: async () => true,
      getPublicKey: async () => 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
      getNetwork: async () => 'TESTNET',
      signTransaction: async () => 'signed-xdr-mock',
    };
  });
}

function mockCctpApi(page: Page) {
  const transferId = 'transfer-e2e-1';
  let burnPhase: 'approval' | 'burn' = 'approval';
  let status = 'burn_prepared';

  return page.route('**/api/v2**', async (route) => {
    const url = route.request().url();
    const method = route.request().method();

    if (url.endsWith('/api/v2') && method === 'GET') {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: jsonData({
          version: 2,
            chain_aware_assets: true,
            bridge_venues_metadata_only: false,
            bridge_settlement_executable: true,
            supported_chain_namespaces: ['stellar', 'eip155'],
            supported_corridors: [
              {
                corridor_id: 'circle-cctp:usdc:stellar-testnet:ethereum-sepolia',
                provider: 'circle-cctp',
                direction: 'evm_to_stellar',
                source_chain_id: 'eip155:11155111',
                destination_chain_id: 'stellar:testnet',
                source_asset: {
                  chain_id: 'eip155:11155111',
                  asset: 'erc20:0x1c7d4b196cb0c7b01d743fbc6116a902379c7238',
                  canonical: 'eip155:11155111/erc20:0x1c7d4b196cb0c7b01d743fbc6116a902379c7238',
                  symbol: 'USDC',
                },
                destination_asset: {
                  chain_id: 'stellar:testnet',
                  asset: 'erc20:CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA',
                  canonical:
                    'stellar:testnet/erc20:CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA',
                  symbol: 'USDC',
                },
                executable: true,
              },
            ],
        }),
      });
    }

    if (url.includes('/bridge/cctp/quote') && method === 'POST') {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: jsonData({
          transfer_id: transferId,
          access_token: 'access-mock-token',
          corridor_id: 'stellar-testnet-sepolia',
          provider: 'circle-cctp',
          direction: 'evm_to_stellar',
          source_amount: '10',
          destination_amount: '9.99',
          fee_quote: {},
          expires_at: Math.floor(Date.now() / 1000) + 600,
          finality: 'standard',
        }),
      });
    }

    if (url.includes('/prepare-burn') && method === 'POST') {
      const approval = burnPhase === 'approval';
      if (!approval) burnPhase = 'burn';
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: jsonData({
          transfer_id: transferId,
          status: 'burn_prepared',
          approval_required: approval,
          expires_at: Math.floor(Date.now() / 1000) + 300,
          payload: {
            type: 'evm_transaction',
            chain_id: 'eip155:11155111',
            to: '0x1c7d4b196cb0c7b01d743fbc6116a902379c7238',
            data: '0x',
            value: '0',
          },
        }),
      });
    }

    if (url.includes('/submit-burn') && method === 'POST') {
      if (burnPhase === 'burn') status = 'awaiting_attestation';
      else burnPhase = 'burn';
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: jsonData({
          transfer_id: transferId,
          status,
          source_tx_hash: '0xdeadbeef',
        }),
      });
    }

    if (url.includes(`/bridge/cctp/${transferId}`) && method === 'GET') {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: jsonData({
          transfer_id: transferId,
          corridor_id: 'stellar-testnet-sepolia',
          provider: 'circle-cctp',
          direction: 'evm_to_stellar',
          status,
          retryable: false,
        }),
      });
    }

    return route.fallback();
  });
}

test.describe('CCTP swap flow (mocked)', () => {
  test.beforeEach(async ({ page }) => {
    await installFakeWallets(page);
    await mockCctpApi(page);
  });

  test('desktop corridor shows deck and hides secrets in DOM', async ({ page }) => {
    await page.goto('/swap');
    await page.waitForSelector('[data-testid="cross-chain-swap-deck"]', {
      timeout: 20_000,
    });
    await page.getByTestId('corridor-tab-evm-to-stellar').click();
    const html = await page.content();
    expect(html).not.toMatch(/access-mock-token/);
    expect(html).not.toMatch(/signed-xdr-mock/);
    await page.screenshot({ path: 'test-results/cctp-deck-desktop.png' });
  });

  test('mobile viewport renders cross-chain deck', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/swap');
    await page.waitForSelector('[data-testid="paired-chain-selectors"]');
    await page.screenshot({ path: 'test-results/cctp-deck-mobile.png' });
  });

  test('one wallet send per approval CTA click', async ({ page }) => {
    await page.goto('/swap');
    await page.waitForSelector('[data-testid="cross-chain-swap-deck"]');
    await page.getByTestId('corridor-tab-evm-to-stellar').click();
    await page.waitForSelector('[data-testid="cctp-source-amount"]', {
      timeout: 20_000,
    });

    await page.getByLabel('Use custom destination recipient').click();
    await page.getByTestId('destination-recipient-input').fill(STELLAR_G);
    await page.getByTestId('cctp-source-amount').fill('10');
    await page.getByTestId('wallet-chip-ethereum-sepolia').click();
    await page.getByRole('button', { name: /EVM Wallet/i }).click();
    await expect(page.getByTestId('cross-chain-review-cta')).toBeEnabled({
      timeout: 10_000,
    });
    await page.getByTestId('cross-chain-review-cta').click();
    await expect(page.getByTestId('cross-chain-review-cta')).toContainText(
      /Approve/i,
      { timeout: 15_000 },
    );
    await page.getByTestId('cross-chain-review-cta').click();

    const sendCount = await page.evaluate(() =>
      (window as unknown as { __cctpWalletSendCount?: () => number }).__cctpWalletSendCount?.(),
    );
    expect(sendCount).toBe(1);
  });
});
