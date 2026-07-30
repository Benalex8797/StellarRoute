# Stellar CCTP verifier status (testnet)

Production Stellar approval, burn, and mint verifiers use Soroban RPC `getTransaction`,
pinned Circle `stellar-cctp` event layouts (`45746f2c8031`), and offline live XDR fixtures.

## Implemented

| Verifier | RPC evidence | On-chain binding |
|----------|--------------|------------------|
| `StellarRpcApprovalVerifier` | Optional standalone `approve` when recorded; allowance simulation | owner, spender=TokenMessenger, amount, expiration ledger |
| `StellarRpcBurnVerifier` | `deposit_for_burn(_with_hook)` invoke + events + `message_sent` | local 7dp→canonical 6dp, max_fee, destination messenger, op-source |
| `StellarRpcMintVerifier` | `mint_and_forward` invoke; completion requires **exactly one** `mint_and_forward` + **exactly one** `message_received` | message+attestation bytes, payload hash, dual-event cross-bindings |

Mint completion **never** succeeds from `is_nonce_used` alone (`ReconciliationNonceConsumed` is
reconciliation-only). `poll_one_transfer` re-queries `MintSubmitted` transfers idempotently.

## Pinned live fixtures (offline CI source of truth)

| Tx | Hash | Ledger |
|----|------|--------|
| Stellar→Sepolia burn | `670c2b7061937108f2e475d68249d1ebf01f089b5309139fbc8806196341860c` | 3867580 |
| Sepolia→Stellar mint | `c59b4c64a993fc317d7ed3ea415f061723b2c67f0e2db01cd3d65028a5c0fdc4` | 3862387 |

Fixtures: `crates/api/src/cctp/fixtures/live_xdr/`. `#[ignore]` tests re-fetch while RPC retention permits.

## Readiness probes (non-mutating)

`stellar_readiness_probes`: RPC reachability, MessageTransmitter `is_nonce_used` simulation,
USDC `decimals`, TokenMessenger `local_domain`, forwarder contract callable.

## Still blocking `is_public_executable`

- Stellar **builders** (`ProductionStellarCctpBuilder`) remain separate readiness gate
- Attestation verifier bootstrap (Iris + attester snapshots) must be ready
- `CCTP_ENABLED` and full `CctpRuntime::assess` must pass for corridor direction

Public HTTP execution wiring is a later reviewed phase.

## Approval uncertainty

No standalone USDC `approve` event observed in recent testnet history; Circle `deposit_for_burn`
may satisfy allowance via Soroban auth in the burn envelope. Standalone approval verifier remains
optional — only runs when `source_approval_tx_hash` is recorded.
