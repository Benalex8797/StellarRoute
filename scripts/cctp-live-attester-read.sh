#!/usr/bin/env bash
# Opt-in read-only live attester enumeration (Sepolia + Stellar testnet).
# Records threshold, enabled count, and set hashes only — no secrets.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export CCTP_ENABLED=false
export SEPOLIA_RPC_URL="${SEPOLIA_RPC_URL:-https://rpc.sepolia.org}"
export STELLAR_RPC_URL="${STELLAR_RPC_URL:-https://soroban-testnet.stellar.org}"

cargo test -p stellarroute-api --lib \
  cctp::evm_attester_reader::live_tests::live_sepolia_enumeration \
  cctp::stellar_attester_reader::live_tests::live_stellar_enumeration \
  -- --ignored --nocapture
