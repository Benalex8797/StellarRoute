# Stellar CCTP verifier blockers (testnet)

Production Stellar burn, approval, and mint verifiers remain **NotReady** until the
following authoritative sources are pinned and implemented:

## Burn verifier (`StellarBurnVerifier`)

**Source:** [circlefin/stellar-cctp](https://github.com/circlefin/stellar-cctp)
`token-messenger-minter-v2/src/deposit.rs`, Circle
[Stellar contracts reference](https://developers.circle.com/cctp/references/stellar-contracts).

**Missing for production implementation:**

1. Pinned Soroban RPC `getTransaction` / event format for `deposit_for_burn` success on
   `TokenMessengerMinter` including: source account, contract id, function name,
   destination domain, burn token, amount (7-decimal Stellar subunits), mint recipient
   bytes32, destination caller, finality threshold, optional hook data.
2. Bounded fixture: successful testnet tx hash + decoded events cross-checked against
   `build_expected_burn_facts` for a known corridor transfer.
3. Horizon/Soroban RPC health gate on exact Testnet passphrase + contract addresses from
   `CctpConfig`.

Without (1), we cannot verify burn facts independently of client-supplied hashes.

## Approval verifier (`StellarApprovalVerifier`)

**Source:** SEP-41 `approve` on Stellar USDC contract
(`CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA`).

**Missing:**

1. Authoritative decode of successful `approve(spender, amount, expiration)` invoke from
   transaction XDR or Soroban simulation result (not just envelope sequence).
2. Allowance read via Soroban contract simulation for `allowance(owner, spender)` with
   exact Testnet RPC URL and ledger expiration.

`source_approval_tx_hash` alone is **never** sufficient; `source_approval_verified_at` is
set only after `StellarApprovalVerifier` returns `VerifiedApprovalFacts`.

## Mint verifier (`StellarMintVerifier`)

**Source:** `CctpForwarder::mint_and_forward(message, attestation)` and MessageTransmitter
nonce-used semantics per Circle Stellar reference.

**Missing:**

1. Pinned decode of successful `mint_and_forward` invoke + MessageTransmitter nonce event
   from Soroban RPC for testnet `CA66Q2WFBND6V4UEB7RD4SAXSVIWMD6RA4X3U32ELVFGXV5PJK4T4VSZ`.
2. Binding recipient G-address from forwarder hook / event evidence.

Until implemented, destination mint on `evm_to_stellar` cannot complete via service path.
