# deploy/secrets.checklist.md
# StellarRoute — Staging Secrets Checklist (Issue #1035 / Wave 0 Oracle)
#
# Work through this list before deploying. All items must be in place
# before the first `up` / public tunnel.

## Preferred free path: Oracle Always Free + Cloudflare Tunnel

See `docs/deployment/oracle-always-free.md` for the full runbook.

Copy `deploy/env.prod.example` → repo-root `.env.prod` and fill:

- [ ] `POSTGRES_USER` / `POSTGRES_PASSWORD` / `POSTGRES_DB`
- [ ] `REDIS_PASSWORD`
- [ ] `SOROBAN_RPC_URL` — testnet: `https://soroban-testnet.stellar.org`
- [ ] `STELLAR_HORIZON_URL` — testnet: `https://horizon-testnet.stellar.org`
- [ ] `ROUTER_CONTRACT_ADDRESS` — from `config/deployments/testnet.json` (`router_contract_id`)
- [ ] `AMM_POOLS` — optional comma-separated pool contract IDs (see `config/pools-testnet.json`) so the indexer can bootstrap reserves when event discovery is empty / outside RPC retention
- [ ] `STELLARROUTE_ENV=production`
- [ ] `CORS_ALLOWED_ORIGINS` — include `https://www.stellarroute.app`, `https://stellarroute.app`, and Vercel production/preview origins as needed
- [ ] `PUBLIC_GET_ROUTES` — keep the defaults from `env.prod.example` for browser GETs
- [ ] `ADMIN_AUTH_TOKEN` — strong random; required when `STELLARROUTE_ENV=production`
- [ ] `ENABLE_ADMIN_ROUTES=false` until kill-switch security review is done
- [ ] Cloudflare Tunnel named hostname (record as `STAGING_API_BASE_URL` / Vercel `NEXT_PUBLIC_API_URL`)
- [ ] `OTEL_EXPORTER_OTLP_ENDPOINT` — optional

Then on the VM:

```bash
docker compose -f docker-compose.yml -f deploy/docker-compose.prod.yml \
  --env-file .env.prod up -d --build
```

## Render deploy (paid / optional Wave 0b)

- [ ] `DATABASE_URL` — automatically wired from `stellarroute-postgres` by `render.yaml`
- [ ] `REDIS_URL` — automatically wired from `stellarroute-redis` by `render.yaml`
- [ ] `SOROBAN_RPC_URL` — set in Render dashboard
- [ ] `STELLAR_HORIZON_URL` — set in Render dashboard
- [ ] `ROUTER_CONTRACT_ADDRESS` — set in Render dashboard
- [ ] `ENABLE_ADMIN_ROUTES` — leave as `false` until security issues are resolved
- [ ] Production CORS / auth vars if exposing the API publicly (`STELLARROUTE_ENV`, `CORS_ALLOWED_ORIGINS`, `ADMIN_AUTH_TOKEN`, `PUBLIC_GET_ROUTES`)
- [ ] `OTEL_EXPORTER_OTLP_ENDPOINT` — optional; set if you have a collector

## Docker Compose (production overlay) — local or any VM

```env
POSTGRES_USER=stellarroute
POSTGRES_PASSWORD=<strong-random-password>
POSTGRES_DB=stellarroute
REDIS_PASSWORD=<strong-random-password>
SOROBAN_RPC_URL=https://soroban-testnet.stellar.org
STELLAR_HORIZON_URL=https://horizon-testnet.stellar.org
ROUTER_CONTRACT_ADDRESS=<deployed-contract-id>
STELLARROUTE_ENV=production
CORS_ALLOWED_ORIGINS=https://www.stellarroute.app,https://stellarroute.app,https://stellarroute-frontend.vercel.app
PUBLIC_GET_ROUTES=/api/v1/quote,/api/v1/pairs,/api/v1/markets,/api/v1/orderbook,/api/v1/routes,/api/v1/price-history,/health
ADMIN_AUTH_TOKEN=<strong-random-token>
ENABLE_ADMIN_ROUTES=false
API_HOST_PORT=8080
```

```bash
cp deploy/env.prod.example .env.prod
# edit .env.prod
docker compose -f docker-compose.yml -f deploy/docker-compose.prod.yml \
  --env-file .env.prod up -d --build
```

## Post-deploy verification

```bash
# On the host (before tunnel)
curl -sf http://127.0.0.1:8080/health && echo "API live locally"

# Public (after Cloudflare Tunnel)
curl -sf https://<tunnel-hostname>/health && echo "API live"
curl -sf https://<tunnel-hostname>/health/deps && echo "API ready"

# Staging smoke (from laptop / CI)
STAGING_API_BASE_URL=https://<tunnel-hostname> ./scripts/staging-smoke.sh
```
