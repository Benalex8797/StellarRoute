# Frontend

Next.js App Router UI for StellarRoute.

## Local development

```bash
cp .env.example .env.local
npm install
npm run dev
```

`NEXT_PUBLIC_API_URL` may point at `http://localhost:8080` in development.

## Production / Vercel

See [`docs/deployment/vercel-frontend.md`](../docs/deployment/vercel-frontend.md).

Production builds enforce a public API URL via `lib/env-guard.ts` when
`VERCEL_ENV=production` or `STELLARROUTE_ENV=production`.

```bash
npm run test -- lib/env-guard.test.ts
VERCEL_ENV=production \
NEXT_PUBLIC_API_URL=https://api.example.com \
NEXT_PUBLIC_STELLAR_NETWORK=testnet \
npm run build
```
