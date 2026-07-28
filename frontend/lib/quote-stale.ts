/**
 * Quote freshness / stale UI timing.
 *
 * Backend caches GET /api/v1/quote responses for **5 seconds**
 * (see docs/api/openapi.yaml). After that window, displayed numbers may
 * diverge from the next server response — show a stale indicator.
 */
export const QUOTE_STALE_AFTER_MS = 5500;

/** Debounce before firing a quote request while the user edits amount. */
export const QUOTE_AMOUNT_DEBOUNCE_MS = 450;

/**
 * Default interval when auto-refresh is enabled (15–30s range per product guidance).
 */
export const QUOTE_AUTO_REFRESH_INTERVAL_MS = 20_000;

/**
 * Minimum spacing between **manual** refresh clicks to avoid hammering the API.
 * Auto-refresh uses {@link QUOTE_AUTO_REFRESH_INTERVAL_MS} instead.
 */
export const QUOTE_MANUAL_REFRESH_COOLDOWN_MS = 2000;

/**
 * Returns true when a successful quote is older than `staleAfterMs` relative to `nowMs`.
 * If `expiresAtMs` is provided **and is still in the future relative to when we
 * received the quote**, it takes precedence over `staleAfterMs`.
 *
 * Cached API responses can arrive with an already-past `expires_at`; treating
 * those as immediately stale bricks the swap CTA ("Session restored" forever).
 * In that case fall back to the client receive-time window.
 */
export function isQuoteStale(
  lastSuccessTimeMs: number | null,
  nowMs: number,
  staleAfterMs: number = QUOTE_STALE_AFTER_MS,
  expiresAtMs?: number,
): boolean {
  if (lastSuccessTimeMs == null) return false;

  if (
    expiresAtMs != null &&
    Number.isFinite(expiresAtMs) &&
    expiresAtMs > lastSuccessTimeMs
  ) {
    return nowMs >= expiresAtMs;
  }

  return nowMs - lastSuccessTimeMs >= staleAfterMs;
}
