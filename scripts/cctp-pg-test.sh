#!/usr/bin/env bash
# Disposable host-port Postgres for CCTP store integration (migrations 0015–0019).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT="${CCTP_PG_PORT:-55432}"
CONTAINER="stellarroute-cctp-pg-test-$$"
export TEST_DATABASE_URL="postgres://stellarroute:stellarroute@127.0.0.1:${PORT}/stellarroute"

cleanup() {
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
}
trap cleanup EXIT

cleanup
docker run -d --name "$CONTAINER" \
  -e POSTGRES_USER=stellarroute \
  -e POSTGRES_PASSWORD=stellarroute \
  -e POSTGRES_DB=stellarroute \
  -p "127.0.0.1:${PORT}:5432" \
  postgres:16-alpine >/dev/null

for _ in $(seq 1 30); do
  if docker exec "$CONTAINER" pg_isready -U stellarroute -d stellarroute >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

for migration in \
  "$ROOT/crates/api/migrations/0015_cctp_transfers.sql" \
  "$ROOT/crates/api/migrations/0016_cctp_transfers_hardening.sql" \
  "$ROOT/crates/api/migrations/0017_cctp_mint_metadata.sql" \
  "$ROOT/crates/api/migrations/0018_cctp_approval_tx_hash.sql" \
  "$ROOT/crates/api/migrations/0019_cctp_approval_verified_at.sql"
do
  docker exec -i "$CONTAINER" psql -U stellarroute -d stellarroute -v ON_ERROR_STOP=1 <"$migration"
done

cd "$ROOT"
cargo test -p stellarroute-api --test cctp_store_integration pg_store_prepare_submit_retry_paths -- --ignored --test-threads=1

echo "PG test passed against $TEST_DATABASE_URL"
