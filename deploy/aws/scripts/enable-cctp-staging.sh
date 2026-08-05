#!/usr/bin/env bash
# Enable CCTP on EC2 staging by upserting .env.prod and recreating the API container.
#
# Usage (on the staging host, from repo root):
#   CCTP_SEPOLIA_RPC_URL=https://sepolia.drpc.org bash deploy/aws/scripts/enable-cctp-staging.sh
#
# Defaults:
#   CCTP_ENABLED=true
#   CCTP_SEPOLIA_RPC_URL / SEPOLIA_RPC_URL required (or pass explicitly)
#   CCTP_ACCESS_TOKEN_HMAC_KEY generated if missing/empty
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "${ROOT}"

ENV_FILE="${ROOT}/.env.prod"
if [[ ! -f "${ENV_FILE}" ]]; then
  echo "Missing ${ENV_FILE}. Copy deploy/env.prod.example and fill required values first." >&2
  exit 1
fi

SEPOLIA_RPC="${CCTP_SEPOLIA_RPC_URL:-${SEPOLIA_RPC_URL:-}}"
if [[ -z "${SEPOLIA_RPC}" ]]; then
  echo "CCTP_SEPOLIA_RPC_URL or SEPOLIA_RPC_URL is required (explicit Sepolia JSON-RPC; no silent default)." >&2
  exit 1
fi

upsert_env() {
  local key="$1"
  local value="$2"
  local tmp
  tmp="$(mktemp)"
  if grep -qE "^${key}=" "${ENV_FILE}"; then
    # Preserve other lines; replace matching key only.
    awk -v k="${key}" -v v="${value}" '
      BEGIN { found=0 }
      index($0, k "=") == 1 { print k "=" v; found=1; next }
      { print }
      END { if (!found) print k "=" v }
    ' "${ENV_FILE}" > "${tmp}"
  else
    cat "${ENV_FILE}" > "${tmp}"
    printf '%s=%s\n' "${key}" "${value}" >> "${tmp}"
  fi
  mv "${tmp}" "${ENV_FILE}"
  chmod 600 "${ENV_FILE}" || true
}

# shellcheck disable=SC1090
set -a
# Load existing values without exporting secrets to logs later.
source "${ENV_FILE}"
set +a

HMAC_KEY="${CCTP_ACCESS_TOKEN_HMAC_KEY:-}"
if [[ -z "${HMAC_KEY}" ]]; then
  HMAC_KEY="$(python3 -c 'import os,base64; print(base64.urlsafe_b64encode(os.urandom(32)).decode().rstrip("="))')"
  echo "Generated new CCTP_ACCESS_TOKEN_HMAC_KEY (not printed)."
else
  echo "Reusing existing CCTP_ACCESS_TOKEN_HMAC_KEY."
fi

upsert_env "CCTP_ENABLED" "true"
upsert_env "CCTP_ACCESS_TOKEN_HMAC_KEY" "${HMAC_KEY}"
upsert_env "CCTP_SEPOLIA_RPC_URL" "${SEPOLIA_RPC}"

if [[ -n "${CCTP_STELLAR_RPC_URL:-}" ]]; then
  upsert_env "CCTP_STELLAR_RPC_URL" "${CCTP_STELLAR_RPC_URL}"
fi
if [[ -n "${CCTP_IRIS_BASE_URL:-}" ]]; then
  upsert_env "CCTP_IRIS_BASE_URL" "${CCTP_IRIS_BASE_URL}"
fi

echo "Recreating API with CCTP enabled..."
docker compose -f docker-compose.yml -f deploy/docker-compose.prod.yml --env-file .env.prod up -d --no-deps api

API_PORT="${API_HOST_PORT:-8080}"
echo "Waiting for API health..."
for _ in $(seq 1 60); do
  if curl -fsS "http://127.0.0.1:${API_PORT}/health" >/dev/null \
    && curl -fsS "http://127.0.0.1:${API_PORT}/health/deps" >/dev/null; then
    echo "API healthy."
    break
  fi
  sleep 5
done

if v2_json="$(curl -sf "http://127.0.0.1:${API_PORT}/api/v2" 2>/dev/null)"; then
  printf '%s' "${v2_json}" | python3 -c '
import json,sys
d=json.load(sys.stdin).get("data",{})
print("bridge_settlement_executable={}".format(d.get("bridge_settlement_executable")))
for c in d.get("supported_corridors") or []:
    print(" corridor direction={} executable={}".format(c.get("direction"), c.get("executable")))
'
else
  echo "WARN: /api/v2 not ready yet" >&2
fi
