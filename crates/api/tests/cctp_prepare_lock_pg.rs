//! Postgres integration tests for CCTP prepare-lock atomicity.
//! Run via `scripts/cctp-pg-test.sh` or `TEST_DATABASE_URL=... cargo test --test cctp_prepare_lock_pg -- --ignored`

use chrono::{Duration, Utc};
use sqlx::postgres::PgPoolOptions;
use stellarroute_api::cctp::prepare_lock::{
    CctpActivePrepare, CctpPrepareKind, CctpPrepareLockStore, InMemoryCctpPrepareLockStore,
    PgCctpPrepareLockStore, PrepareAcquireResult,
};
use stellarroute_api::cctp::store::{CctpTransfer, CctpTransferStore, PgCctpTransferStore};
use stellarroute_api::models::v2_cctp::{CctpDirection, CctpFinality, CctpTransferStatus};
use uuid::Uuid;

fn sample_transfer(id: Uuid, sender: &str) -> CctpTransfer {
    let now = Utc::now();
    CctpTransfer {
        transfer_id: id,
        support_reference_id: "sup-lock".into(),
        corridor_id: "c".into(),
        provider: "circle-cctp".into(),
        direction: CctpDirection::StellarToEvm,
        source_chain_id: "stellar:testnet".into(),
        destination_chain_id: "eip155:11155111".into(),
        source_asset: "a".into(),
        source_asset_canonical: "a".into(),
        destination_asset: "b".into(),
        destination_asset_canonical: "b".into(),
        sender: sender.into(),
        recipient: "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0".into(),
        mint_submitter: None,
        amount: "10".into(),
        destination_amount: "10".into(),
        finality: CctpFinality::Standard,
        runtime_fee_quote: None,
        max_fee: Some("1".into()),
        fee_expires_at: Some(now + Duration::minutes(10)),
        quote_expires_at: now + Duration::minutes(10),
        status: CctpTransferStatus::Created,
        source_tx_hash: None,
        source_approval_tx_hash: None,
        source_approval_verified_at: None,
        destination_tx_hash: None,
        iris_message_hash: None,
        message_nonce: None,
        raw_message: None,
        attestation: None,
        retry_count: 0,
        last_provider_error: None,
        last_provider_code: None,
        version: 1,
        created_at: now,
        updated_at: now,
        terminal_at: None,
        mint_payload_hash: None,
        mint_payload_expires_at: None,
        approval_payload_hash: None,
        approval_expiration_ledger: None,
        burn_payload_hash: None,
        burn_prepare_step: None,
    }
}

fn reservation(source: &str, transfer_id: Uuid, hash: &str, payload: &str) -> CctpActivePrepare {
    CctpActivePrepare {
        source_account: source.into(),
        transfer_id,
        kind: CctpPrepareKind::Burn,
        payload_hash: hash.into(),
        prepared_payload: Some(payload.into()),
        expires_at: Utc::now() + Duration::minutes(5),
        updated_at: Utc::now(),
    }
}

async fn pg_pool_from_env() -> sqlx::PgPool {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect")
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL with CCTP migrations applied"]
async fn pg_same_transfer_idempotent() {
    let pool = pg_pool_from_env().await;
    let store = PgCctpTransferStore::new(pool.clone());
    let locks = PgCctpPrepareLockStore::new(pool);
    let source = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
    let tid = Uuid::new_v4();
    store.insert(&sample_transfer(tid, source)).await.unwrap();
    let r1 = reservation(source, tid, "hash-a", "payload-a");
    assert!(matches!(
        locks.try_acquire(&r1).await.unwrap(),
        PrepareAcquireResult::Acquired
    ));
    let r2 = reservation(source, tid, "hash-a", "payload-a");
    assert!(matches!(
        locks.try_acquire(&r2).await.unwrap(),
        PrepareAcquireResult::Idempotent(_)
    ));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL with CCTP migrations applied"]
async fn pg_conflict_other_transfer() {
    let pool = pg_pool_from_env().await;
    let store = PgCctpTransferStore::new(pool.clone());
    let locks = PgCctpPrepareLockStore::new(pool);
    let source = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
    let t1 = Uuid::new_v4();
    let t2 = Uuid::new_v4();
    store.insert(&sample_transfer(t1, source)).await.unwrap();
    store.insert(&sample_transfer(t2, source)).await.unwrap();
    locks
        .try_acquire(&reservation(source, t1, "a", "p1"))
        .await
        .unwrap();
    assert!(matches!(
        locks
            .try_acquire(&reservation(source, t2, "b", "p2"))
            .await
            .unwrap(),
        PrepareAcquireResult::ConflictOtherTransfer { .. }
    ));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL with CCTP migrations applied"]
async fn pg_wrong_transfer_release_noop() {
    let pool = pg_pool_from_env().await;
    let store = PgCctpTransferStore::new(pool.clone());
    let locks = PgCctpPrepareLockStore::new(pool);
    let source = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
    let tid = Uuid::new_v4();
    store.insert(&sample_transfer(tid, source)).await.unwrap();
    locks
        .try_acquire(&reservation(source, tid, "a", "p"))
        .await
        .unwrap();
    assert!(!locks.release(source, Uuid::new_v4()).await.unwrap());
    assert!(locks.get_active(source).await.unwrap().is_some());
}

#[tokio::test]
async fn memory_matches_pg_semantics_smoke() {
    let mem = InMemoryCctpPrepareLockStore::default();
    let source = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
    let tid = Uuid::new_v4();
    mem.try_acquire(&reservation(source, tid, "h", "p"))
        .await
        .unwrap();
    assert!(matches!(
        mem.try_acquire(&reservation(source, tid, "h", "p"))
            .await
            .unwrap(),
        PrepareAcquireResult::Idempotent(_)
    ));
}
