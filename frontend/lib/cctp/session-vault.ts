const VAULT_KEY = 'stellarroute:cctp:v1';
const VAULT_VERSION = 1;
const DEFAULT_TTL_MS = 2 * 60 * 60 * 1000;

export interface CctpSessionRecoveryMeta {
  corridorId: string;
  direction: 'stellar_to_evm' | 'evm_to_stellar';
  sourceChainId: string;
  destChainId: string;
  amount: string;
  recipient: string;
  quoteExpiresAt?: number;
}

export interface CctpSessionRecord {
  version: typeof VAULT_VERSION;
  transferId: string;
  accessToken: string;
  idempotencyKey: string;
  createdAt: number;
  expiresAt: number;
  recovery: CctpSessionRecoveryMeta;
}

export type CctpSessionLoadResult =
  | { ok: true; record: CctpSessionRecord }
  | { ok: false; reason: 'missing' | 'invalid' | 'expired' | 'terminal' };

const TERMINAL_PURGE_STATUSES = new Set([
  'completed',
  'cancelled',
  'provider_killed',
]);

function isBrowserSession(): boolean {
  return typeof window !== 'undefined' && typeof sessionStorage !== 'undefined';
}

function validateRecord(raw: unknown): CctpSessionRecord | null {
  if (!raw || typeof raw !== 'object') return null;
  const r = raw as Partial<CctpSessionRecord>;
  if (r.version !== VAULT_VERSION) return null;
  if (!r.transferId || typeof r.transferId !== 'string') return null;
  if (!r.accessToken || typeof r.accessToken !== 'string') return null;
  if (!r.idempotencyKey || typeof r.idempotencyKey !== 'string') return null;
  if (typeof r.createdAt !== 'number' || typeof r.expiresAt !== 'number') {
    return null;
  }
  if (!r.recovery || typeof r.recovery !== 'object') return null;
  const rec = r.recovery as Partial<CctpSessionRecoveryMeta>;
  if (!rec.corridorId || !rec.direction || !rec.amount || !rec.recipient) {
    return null;
  }
  return r as CctpSessionRecord;
}

export function saveCctpSession(record: CctpSessionRecord): void {
  if (!isBrowserSession()) return;
  sessionStorage.setItem(VAULT_KEY, JSON.stringify(record));
}

export function loadCctpSession(now = Date.now()): CctpSessionLoadResult {
  if (!isBrowserSession()) {
    return { ok: false, reason: 'missing' };
  }
  const raw = sessionStorage.getItem(VAULT_KEY);
  if (!raw) return { ok: false, reason: 'missing' };
  try {
    const parsed = validateRecord(JSON.parse(raw));
    if (!parsed) {
      clearCctpSession();
      return { ok: false, reason: 'invalid' };
    }
    if (parsed.expiresAt <= now) {
      clearCctpSession();
      return { ok: false, reason: 'expired' };
    }
    return { ok: true, record: parsed };
  } catch {
    clearCctpSession();
    return { ok: false, reason: 'invalid' };
  }
}

export function clearCctpSession(): void {
  if (!isBrowserSession()) return;
  sessionStorage.removeItem(VAULT_KEY);
}

export function purgeCctpSessionIfTerminal(status: string): void {
  if (TERMINAL_PURGE_STATUSES.has(status)) {
    clearCctpSession();
  }
}

export function buildCctpSessionRecord(input: {
  transferId: string;
  accessToken: string;
  idempotencyKey: string;
  recovery: CctpSessionRecoveryMeta;
  quoteExpiresAt?: number;
  ttlMs?: number;
  now?: number;
}): CctpSessionRecord {
  const now = input.now ?? Date.now();
  const ttl = input.ttlMs ?? DEFAULT_TTL_MS;
  const quoteExpiryMs = input.quoteExpiresAt
    ? input.quoteExpiresAt * 1000
    : now + ttl;
  return {
    version: VAULT_VERSION,
    transferId: input.transferId,
    accessToken: input.accessToken,
    idempotencyKey: input.idempotencyKey,
    createdAt: now,
    expiresAt: Math.min(now + ttl, quoteExpiryMs + 30 * 60 * 1000),
    recovery: input.recovery,
  };
}

/** Safe snapshot for UI — never includes access token. */
export function cctpSessionPublicView(
  record: CctpSessionRecord,
): Omit<CctpSessionRecord, 'accessToken'> & { hasToken: true } {
  const { accessToken: _token, ...rest } = record;
  return { ...rest, hasToken: true };
}

export function redactSecretsForLogs(value: unknown): string {
  const json = JSON.stringify(value);
  return json
    .replace(/"access_token"\s*:\s*"[^"]+"/gi, '"access_token":"[redacted]"')
    .replace(/"accessToken"\s*:\s*"[^"]+"/gi, '"accessToken":"[redacted]"');
}
