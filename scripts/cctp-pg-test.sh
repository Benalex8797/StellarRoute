#!/usr/bin/env bash
# Disposable Postgres for CCTP store + prepare-lock integration tests.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONTAINER="stellarroute-cctp-pg-test-$$"
PORT="${CCTP_PG_PORT:-55432}"
export TEST_DATABASE_URL="postgres://postgres:postgres@127.0.0.1:${PORT}/stellarroute_cctp_test"

cleanup() {
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
}
trap cleanup EXIT

cleanup
docker run -d --name "$CONTAINER" -e POSTGRES_PASSWORD=postgres -p "${PORT}:5432" postgres:16-alpine >/dev/null

for _ in $(seq 1 60); do
  if docker exec "$CONTAINER" pg_isready -U postgres >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

docker exec "$CONTAINER" psql -U postgres -c "CREATE DATABASE stellarroute_cctp_test;" >/dev/null

apply_migration() {
  local file="$1"
  echo "Applying $(basename "$file")"
  docker exec -i "$CONTAINER" psql -U postgres -d stellarroute_cctp_test -v ON_ERROR_STOP=1 <"$file"
}

MIG_DIR="$ROOT/crates/api/migrations"
for f in \
  "$MIG_DIR/0015_cctp_transfers.sql" \
  "$MIG_DIR/0016_cctp_transfers_hardening.sql" \
  "$MIG_DIR/0017_cctp_mint_metadata.sql" \
  "$MIG_DIR/0018_cctp_approval_tx_hash.sql" \
  "$MIG_DIR/0019_cctp_approval_verified_at.sql" \
  "$MIG_DIR/20260730_cctp_review_fixes.sql" \
  "$MIG_DIR/20260731_cctp_prepare_lock_hardening.sql"
do
  apply_migration "$f"
done

cd "$ROOT"
echo "Running cctp_store_integration..."
cargo test -p stellarroute-api --test cctp_store_integration -- --ignored --nocapture

echo "Running cctp_prepare_lock_pg..."
cargo test -p stellarroute-api --test cctp_prepare_lock_pg -- --ignored --nocapture

echo "CCTP PG tests passed against $TEST_DATABASE_URL"
