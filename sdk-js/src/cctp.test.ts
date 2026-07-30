import { describe, expect, it, vi, afterEach } from 'vitest';
import {
  StellarRouteClient,
  StellarRouteApiError,
  isStellarRouteApiError,
  parseApiErrorBody,
} from './client.js';
import {
  API_ERROR_CODES,
  CCTP_PROVIDER_ID,
  CCTP_TESTNET_CORRIDOR_ID,
  type CctpQuoteRequest,
} from './types.js';

const TRANSFER_ID = '550e8400-e29b-41d4-a716-446655440000';

const sampleQuoteRequest: CctpQuoteRequest = {
  corridor_id: CCTP_TESTNET_CORRIDOR_ID,
  provider: CCTP_PROVIDER_ID,
  direction: 'evm_to_stellar',
  source_chain_id: 'eip155:11155111',
  destination_chain_id: 'stellar:testnet',
  source_asset: {
    chain_id: 'eip155:11155111',
    asset: 'erc20:0x1c7d4b196cb0c7b01d743fbc6116a902379c7238',
    canonical:
      'eip155:11155111/erc20:0x1c7d4b196cb0c7b01d743fbc6116a902379c7238',
    symbol: 'USDC',
  },
  destination_asset: {
    chain_id: 'stellar:testnet',
    asset: 'erc20:CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA',
    canonical:
      'stellar:testnet/erc20:CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA',
    symbol: 'USDC',
  },
  amount: '100.000000',
  recipient: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
  finality: 'standard',
};

function envelopeError(code: string, message: string, status: number): Response {
  return new Response(
    JSON.stringify({
      v: 2,
      request_id: 'req-test',
      data: { error: code, message },
    }),
    { status, headers: { 'Content-Type': 'application/json' } },
  );
}

describe('CCTP SDK contract', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('API_ERROR_CODES includes CCTP taxonomy entries', () => {
    for (const code of [
      'cctp_not_enabled',
      'unsupported_corridor',
      'invalid_finality',
      'invalid_recipient',
      'fee_quote_unavailable',
      'attestation_pending',
      'attestation_expired',
      'mint_retryable',
      'transfer_not_found',
      'provider_killed',
    ]) {
      expect(API_ERROR_CODES).toContain(code);
    }
  });

  it('parseApiErrorBody reads nested envelope cctp_not_enabled', () => {
    const parsed = parseApiErrorBody({
      v: 2,
      data: {
        error: 'cctp_not_enabled',
        message: 'disabled',
      },
    });
    expect(parsed.error).toBe('cctp_not_enabled');
    expect(parsed.message).toBe('disabled');
  });

  it('cctpQuote POSTs to /api/v2/bridge/cctp/quote and surfaces 503', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      envelopeError('cctp_not_enabled', 'not enabled', 503),
    );
    vi.stubGlobal('fetch', fetchMock);

    const client = new StellarRouteClient({ baseUrl: 'http://api.test', retries: 0 });
    await expect(client.cctpQuote(sampleQuoteRequest)).rejects.toSatisfy(
      (err: unknown) =>
        isStellarRouteApiError(err) &&
        err.code === 'cctp_not_enabled' &&
        err.status === 503,
    );

    expect(fetchMock).toHaveBeenCalledOnce();
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('http://api.test/api/v2/bridge/cctp/quote');
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body as string)).toEqual(sampleQuoteRequest);
  });

  it('cctpGetTransfer GETs encoded transfer path', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      envelopeError('cctp_not_enabled', 'not enabled', 503),
    );
    vi.stubGlobal('fetch', fetchMock);

    const client = new StellarRouteClient({ baseUrl: 'http://api.test', retries: 0 });
    await expect(client.cctpGetTransfer(TRANSFER_ID)).rejects.toBeInstanceOf(
      StellarRouteApiError,
    );

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(
      `http://api.test/api/v2/bridge/cctp/${TRANSFER_ID}`,
    );
    expect(init.method).toBe('GET');
  });

  it('cctpSubmitBurn serializes tx_hash acknowledgement body', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      envelopeError('cctp_not_enabled', 'not enabled', 503),
    );
    vi.stubGlobal('fetch', fetchMock);

    const client = new StellarRouteClient({ baseUrl: 'http://api.test', retries: 0 });
    await expect(
      client.cctpSubmitBurn(TRANSFER_ID, { tx_hash: '0xabc' }),
    ).rejects.toBeInstanceOf(StellarRouteApiError);

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(JSON.parse(init.body as string)).toEqual({ tx_hash: '0xabc' });
  });

  it('getApiV2Info unwraps supported_corridors array', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          v: 2,
          data: {
            version: 2,
            chain_aware_assets: true,
            bridge_venues_metadata_only: true,
            bridge_settlement_executable: false,
            supported_chain_namespaces: ['stellar', 'eip155'],
            supported_corridors: [],
          },
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      ),
    );
    vi.stubGlobal('fetch', fetchMock);

    const client = new StellarRouteClient({ baseUrl: 'http://api.test', retries: 0 });
    const info = await client.getApiV2Info();
    expect(info.bridge_settlement_executable).toBe(false);
    expect(info.supported_corridors).toEqual([]);
  });
});
