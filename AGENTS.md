# AGENTS.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## What this repo is
StellarRoute is a Rust-first Stellar DEX aggregator with:
- an indexer (`crates/indexer`) that ingests SDEX + Soroban AMM state into Postgres,
- an API (`crates/api`) that serves quotes/orderbooks/routes and optional Redis-backed caching,
- a routing engine (`crates/routing`) used by API logic,
- Soroban contracts (`crates/contracts`),
- a Next.js frontend (`frontend`) and TypeScript SDK (`sdk-js`).

## Common commands
Use these commands from repo root unless noted.

### Local dependencies
- Start Postgres + Redis (deps only):
  - `docker-compose up -d`
- Start full stack (Postgres + Redis + API):
  - `docker compose -f docker-compose.yml -f docker-compose.app.yml up -d`
- Start full stack with indexer (requires `ROUTER_CONTRACT_ADDRESS` in `.env`):
  - `docker compose -f docker-compose.yml -f docker-compose.app.yml --profile indexer up -d`
- Start full stack with frontend UI:
  - `docker compose -f docker-compose.yml -f docker-compose.app.yml --profile ui up -d`
- Wait for service health (deps only):
  - `./scripts/wait-for-services.sh`
- Wait for service health (deps + API):
  - `./scripts/wait-for-services.sh --api`
- Wait for databases to be healthy:
  - `./scripts/wait-for-dbs.sh`
- Check service health:
  - `docker-compose ps`

### Rust workspace
- Build all crates:
  - `cargo build`
- Run all tests:
  - `cargo test`
- Run formatting check (same as CI):
  - `cargo fmt --all -- --check`
- Run clippy (same as CI):
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo clippy -p stellarroute-contracts --all-targets -- -D warnings`
- Run a single test (example pattern):
  - `cargo test -p stellarroute-api quote::tests::selects_best_executable_direct_venue`
  - `cargo test -p stellarroute-routing pathfinder::tests::...`
- Run ignored/integration-style tests when needed:
  - `cargo test -- --include-ignored`

### Run services
- API server:
  - `cargo run -p stellarroute-api`
- Indexer:
  - `cargo run -p stellarroute-indexer`

### Frontend (`frontend/`)
- Install deps:
  - `npm --prefix frontend install`
- Dev server:
  - `npm --prefix frontend run dev`
- Build:
  - `npm --prefix frontend run build`
- Lint:
  - `npm --prefix frontend run lint`
- Unit tests:
  - `npm --prefix frontend run test`
- Single test file / test name:
  - `npm --prefix frontend run test -- src/path/to/file.test.tsx -t "test name"`
- E2E:
  - `npm --prefix frontend run test:e2e`
- Story snapshot build:
  - `npm --prefix frontend run storybook:ci`

### JS SDK (`sdk-js/`)
- Install deps:
  - `npm --prefix sdk-js install`
- Build:
  - `npm --prefix sdk-js run build`
- Test:
  - `npm --prefix sdk-js run test`
- Single test file / test name:
  - `npm --prefix sdk-js run test -- src/path/to/file.test.ts -t "test name"`
- Typecheck/lint:
  - `npm --prefix sdk-js run typecheck`

## Required runtime configuration
- API requires `DATABASE_URL`; optional `REDIS_URL`.
- Indexer requires `DATABASE_URL`, `STELLAR_HORIZON_URL`, `SOROBAN_RPC_URL`, and `ROUTER_CONTRACT_ADDRESS`.
- Typical local values are documented in `docs/development/SETUP.md`.

## Big-picture architecture and execution flow
Focus here first when debugging behavior across crates.

1. Data ingestion and normalization
- `crates/indexer/src/bin/stellarroute-indexer.rs` boots DB, runs migrations, then starts:
  - SDEX loop (`sdex.rs`) reading Horizon offers,
  - AMM loop (`amm.rs`) reading Soroban events/pool state,
  - maintenance loop (snapshot compaction, retention cleanup, materialized view refresh).
- Ingestion writes into `assets`, `sdex_offers`, `amm_pool_reserves`, and supporting tables/functions.
- Quote/routing read path is unified via `normalized_liquidity` (see `docs/architecture/database-schema.md`).

2. API request path
- `crates/api/src/bin/stellarroute-api.rs` configures DB pool guardrails, optional startup dependency checks, and launches `Server`.
- `crates/api/src/server.rs` wires middleware (request ID, versioning headers, rate limiting, tracing), routes, Swagger UI, and optional Redis cache.
- `crates/api/src/routes/mod.rs` exposes primary endpoints:
  - `/api/v1/pairs`, `/api/v1/orderbook/:base/:quote`, `/api/v1/quote/:base/:quote`, `/api/v1/routes/:base/:quote`, plus replay/admin/metrics.
- `crates/api/src/routes/quote.rs` is the key quote pipeline:
  - loads candidates from `normalized_liquidity`,
  - applies freshness/health/policy filters from `stellarroute-routing::health::*`,
  - chooses best executable venue,
  - records metrics/tracing and caches short-TTL quote results.

3. Routing engine role
- `crates/routing` is shared routing/health logic (pathfinder, optimizer, risk/policy, consensus, anomaly/freshness/health modules).
- API currently uses routing health + policy components directly for venue filtering/scoring in quote computation.

4. Contracts and SDKs
- `crates/contracts` contains Soroban router-related contracts and tests.
- `sdk-js` wraps API endpoints for external clients; examples in `sdk-js/examples/`.
- `crates/sdk-rust` is the Rust SDK workspace member.

## High-value files to open first
- `crates/indexer/src/bin/stellarroute-indexer.rs`
- `crates/indexer/src/sdex.rs`
- `crates/indexer/src/amm.rs`
- `crates/api/src/bin/stellarroute-api.rs`
- `crates/api/src/server.rs`
- `crates/api/src/routes/quote.rs`
- `crates/api/src/state.rs`
- `crates/routing/src/lib.rs`
- `docs/architecture/database-schema.md`

## Known project-specific testing details
- Frontend Vitest setup includes `matchMedia` and `localStorage` mocks in `frontend/vitest.setup.ts`.
- If icon imports break frontend tests, check `frontend/__mocks__/lucide-react.tsx`.

## Learned User Preferences
- Primary goal is a live DEX with real users: prioritize production deployability over docs-only or filler work.
- GitHub issues should be grounded in real codebase gaps (not placeholders), with hard/high-quality acceptance criteria and Wave-friendly labels.
- Frontend contributor issues should require downloading relevant frontend skills, and should cover a premium UI plus Vercel and testnet deployment work.
- Frontend UI should feel unique and spacious; reject dense/jammed header and swap chrome, and polish wallet/error messaging rather than stacking warnings.
- When processing contributor/fork PRs, fix conflicts and CI and merge rather than closing; keep going until the open queue is empty unless a PR is explicitly unmergeable.
- Prefer lean CI that contributors can get green easily; remove or simplify unnecessary checks when CI is blocking merges.
- For large PR queues, prefer parallel per-PR workers over a single serial queue.
- When closing multiple related issues, prefer one PR that closes them together.
- Do not edit attached plan files during implementation.
- Prefer free always-on Wave 0 staging (Oracle Always Free + Cloudflare Tunnel) over paid Render when cost matters; a public HTTPS API is required for Freighter/Vercel (localhost alone is not enough).

## Learned Workspace Facts
- Canonical GitHub repo is `StellarRoute/StellarRoute`; local path is `/Users/daniel/Desktop/2026/StellarRoute`.
- Project participates in the Drips/Stellar Wave contributor program; issues commonly use Wave/`help wanted`/complexity labels.
- Frontend production is on Vercel (`stellarroute.app` and `www.stellarroute.app`); GitHub-linked auto-deploy from `main` with root directory `frontend`; API CORS and env allowlists should include both hosts; wiring to a public testnet API/indexer is an explicit product goal.
- Browser wallet support is Freighter, xBull, Albedo, and LOBSTR; Freighter detection should use `isConnected()`, not `isAllowed`.
- Wave 0 public testnet API path is Oracle Always Free ARM VM + `deploy/docker-compose.prod.yml` + Cloudflare Tunnel; runbook is `docs/deployment/oracle-always-free.md` (paid Render blueprint remains optional later).
- Related sibling work under `~/Desktop/2026/` (separate from this repo) includes StellarHydra, WaveFlow, route-visualizer, and swap-agrregrator — do not commit StellarRoute changes into those trees by mistake.
- Frontend Vitest in CI is split by path (app/components/hooks/lib); flaky or heavy suites have been a recurring main-branch blocker.
- `gh` is the expected interface for GitHub issues, PRs, labels, and CI log inspection on this repo.
