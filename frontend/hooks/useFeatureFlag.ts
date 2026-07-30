'use client';

import { useEffect, useState } from 'react';

export type FlagName =
  | "routes_beta"
  | "batch_swaps"
  | "swap_ui_v2"
  | "transaction_history"
  | "advanced_slippage"
  | "real_xdr"
  | "analytics";

export type FlagMap = Partial<Record<FlagName, boolean>>;

/** Security-critical: secure API swap path — not remotely killable. */
export const SECURITY_PINNED_FLAGS: ReadonlySet<FlagName> = new Set(['real_xdr']);

// Cache layer
let remoteFlags: FlagMap | null = null;
let remoteFetchPromise: Promise<FlagMap> | null = null;

export function invalidateFlagCache(): void {
  remoteFlags = null;
  remoteFetchPromise = null;
}

function readEnvFlag(flag: FlagName): boolean | undefined {
  // Static property access is required for Next.js to expose public env values
  // in the browser bundle.
  const val =
    flag === 'routes_beta'
      ? process.env.NEXT_PUBLIC_FLAG_ROUTES_BETA
      : flag === 'batch_swaps'
        ? process.env.NEXT_PUBLIC_FLAG_BATCH_SWAPS
        : flag === 'swap_ui_v2'
          ? process.env.NEXT_PUBLIC_FLAG_SWAP_UI_V2
          : flag === 'transaction_history'
            ? process.env.NEXT_PUBLIC_FLAG_TRANSACTION_HISTORY
            : flag === 'real_xdr'
              ? process.env.NEXT_PUBLIC_FLAG_REAL_XDR
              : flag === 'analytics'
                ? process.env.NEXT_PUBLIC_FEATURE_ANALYTICS
                : process.env.NEXT_PUBLIC_FLAG_ADVANCED_SLIPPAGE;
  if (val === undefined) return undefined;
  return val === 'true' || val === '1';
}

function readWindowFlag(flag: FlagName): boolean | undefined {
  if (typeof window === 'undefined') return undefined;
  const flags = (window as { __STELLAR_ROUTE_FLAGS__?: FlagMap })
    .__STELLAR_ROUTE_FLAGS__;
  if (flags?.[flag] !== undefined) return flags[flag]!;
  return undefined;
}

async function fetchRemoteFlags(): Promise<FlagMap> {
  if (remoteFlags !== null) return remoteFlags;
  if (remoteFetchPromise) return remoteFetchPromise;

  const url = process.env.NEXT_PUBLIC_FLAGS_URL;
  if (!url) return {};

  remoteFetchPromise = fetch(url)
    .then((res) => {
      if (!res.ok) throw new Error(`Flags fetch failed: ${res.status}`);
      return res.json() as Promise<FlagMap>;
    })
    .then((data) => {
      remoteFlags = data;
      return data;
    })
    .catch(() => {
      remoteFlags = {};
      return {};
    });

  return remoteFetchPromise;
}

/**
 * Resolve a flag. Ordinary flags: remote > env > false.
 * `real_xdr` is security-pinned: env/default only (default on). Remote
 * `FLAGS_URL` cannot disable the secure API prepare/sign/submit path.
 */
export function resolveFlag(flag: FlagName, remote: FlagMap = {}): boolean {
  if (SECURITY_PINNED_FLAGS.has(flag)) {
    const env = readEnvFlag(flag);
    if (env !== undefined) return env;
    // Product default: API prepare → wallet sign → API submit.
    return true;
  }
  if (remote[flag] !== undefined) return remote[flag]!;
  const windowFlag = readWindowFlag(flag);
  if (windowFlag !== undefined) return windowFlag;
  const env = readEnvFlag(flag);
  if (env !== undefined) return env;
  return false;
}

function hasLocalFlagResolution(flag: FlagName): boolean {
  return readEnvFlag(flag) !== undefined || readWindowFlag(flag) !== undefined;
}

function initialFlagLoading(flag: FlagName): boolean {
  if (SECURITY_PINNED_FLAGS.has(flag)) return false;
  if (hasLocalFlagResolution(flag)) return false;
  return Boolean(process.env.NEXT_PUBLIC_FLAGS_URL);
}

export function useFeatureFlag(flag: FlagName): {
  enabled: boolean;
  loading: boolean;
} {
  // Security-pinned flags resolve from env/default synchronously so loading
  // never briefly reports enabled=false (which would fail-closed and flash).
  const pinned = SECURITY_PINNED_FLAGS.has(flag);
  const [enabled, setEnabled] = useState<boolean>(() => resolveFlag(flag));
  const [loading, setLoading] = useState<boolean>(() => initialFlagLoading(flag));

  useEffect(() => {
    let cancelled = false;

    if (SECURITY_PINNED_FLAGS.has(flag)) {
      setEnabled(resolveFlag(flag));
      setLoading(false);
      return;
    }

    fetchRemoteFlags().then((remote) => {
      if (!cancelled) {
        setEnabled(resolveFlag(flag, remote));
        setLoading(false);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [flag]);

  return { enabled, loading };
}

export function useFeatureFlags(flags: FlagName[]): Record<FlagName, boolean> {
  const [resolved, setResolved] = useState<Record<FlagName, boolean>>(
    () =>
      Object.fromEntries(
        flags.map((f) => [
          f,
          SECURITY_PINNED_FLAGS.has(f) ? resolveFlag(f) : false,
        ]),
      ) as Record<FlagName, boolean>,
  );

  useEffect(() => {
    let cancelled = false;

    fetchRemoteFlags().then((remote) => {
      if (!cancelled) {
        setResolved(
          Object.fromEntries(
            flags.map((f) => [f, resolveFlag(f, remote)])
          ) as Record<FlagName, boolean>
        );
      }
    });

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [flags.join(',')]);

  return resolved;
}
