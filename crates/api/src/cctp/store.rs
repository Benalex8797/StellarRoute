//! CCTP transfer persistence and optimistic state transitions.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;
use uuid::Uuid;

use crate::cctp::transitions::{is_allowed_transition, is_terminal};
use crate::models::v2_cctp::{CctpDirection, CctpFinality, CctpTransferStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CctpTransfer {
    pub transfer_id: Uuid,
    pub support_reference_id: String,
    pub corridor_id: String,
    pub provider: String,
    pub direction: CctpDirection,
    pub source_chain_id: String,
    pub destination_chain_id: String,
    pub source_asset: String,
    pub source_asset_canonical: String,
    pub destination_asset: String,
    pub destination_asset_canonical: String,
    pub sender: String,
    pub recipient: String,
    pub amount: String,
    pub destination_amount: String,
    pub finality: CctpFinality,
    pub runtime_fee_quote: Option<String>,
    pub max_fee: Option<String>,
    pub fee_expires_at: Option<DateTime<Utc>>,
    pub quote_expires_at: DateTime<Utc>,
    pub status: CctpTransferStatus,
    pub source_tx_hash: Option<String>,
    pub destination_tx_hash: Option<String>,
    pub iris_message_hash: Option<String>,
    pub message_nonce: Option<String>,
    pub raw_message: Option<Vec<u8>>,
    pub attestation: Option<Vec<u8>>,
    pub retry_count: u32,
    pub last_provider_error: Option<String>,
    pub last_provider_code: Option<String>,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub terminal_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Error)]
pub enum CctpStoreError {
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("not found")]
    NotFound,
    #[error("invalid transition")]
    InvalidTransition,
    #[error("duplicate source tx hash")]
    DuplicateSourceTxHash,
    #[error("version conflict")]
    VersionConflict,
}

#[async_trait]
pub trait CctpTransferStore: Send + Sync {
    async fn insert(&self, transfer: &CctpTransfer) -> Result<(), CctpStoreError>;

    async fn get(&self, transfer_id: Uuid) -> Result<Option<CctpTransfer>, CctpStoreError>;

    async fn transition(
        &self,
        transfer_id: Uuid,
        expected_version: i32,
        new_status: CctpTransferStatus,
        patch: TransferPatch,
    ) -> Result<CctpTransfer, CctpStoreError>;
}

#[derive(Debug, Clone, Default)]
pub struct TransferPatch {
    pub source_tx_hash: Option<String>,
    pub destination_tx_hash: Option<String>,
    pub iris_message_hash: Option<String>,
    pub message_nonce: Option<String>,
    pub raw_message: Option<Vec<u8>>,
    pub attestation: Option<Vec<u8>>,
    pub runtime_fee_quote: Option<String>,
    pub max_fee: Option<String>,
    pub fee_expires_at: Option<DateTime<Utc>>,
    pub last_provider_error: Option<String>,
    pub last_provider_code: Option<String>,
    pub increment_retry: bool,
}

pub struct PgCctpTransferStore {
    pool: PgPool,
}

impl PgCctpTransferStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CctpTransferStore for PgCctpTransferStore {
    async fn insert(&self, transfer: &CctpTransfer) -> Result<(), CctpStoreError> {
        sqlx::query(
            r#"
            INSERT INTO cctp_transfers (
                transfer_id, support_reference_id, corridor_id, provider, direction,
                source_chain_id, destination_chain_id, source_asset, source_asset_canonical,
                destination_asset, destination_asset_canonical, sender, recipient,
                amount, destination_amount, finality, runtime_fee_quote, max_fee,
                fee_expires_at, quote_expires_at, status, version
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22
            )
            "#,
        )
        .bind(transfer.transfer_id)
        .bind(&transfer.support_reference_id)
        .bind(&transfer.corridor_id)
        .bind(&transfer.provider)
        .bind(direction_str(transfer.direction))
        .bind(&transfer.source_chain_id)
        .bind(&transfer.destination_chain_id)
        .bind(&transfer.source_asset)
        .bind(&transfer.source_asset_canonical)
        .bind(&transfer.destination_asset)
        .bind(&transfer.destination_asset_canonical)
        .bind(&transfer.sender)
        .bind(&transfer.recipient)
        .bind(&transfer.amount)
        .bind(&transfer.destination_amount)
        .bind(finality_str(transfer.finality))
        .bind(&transfer.runtime_fee_quote)
        .bind(&transfer.max_fee)
        .bind(transfer.fee_expires_at)
        .bind(transfer.quote_expires_at)
        .bind(status_str(transfer.status))
        .bind(transfer.version)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, transfer_id: Uuid) -> Result<Option<CctpTransfer>, CctpStoreError> {
        let row = sqlx::query_as::<_, TransferRow>(
            r#"
            SELECT * FROM cctp_transfers WHERE transfer_id = $1
            "#,
        )
        .bind(transfer_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(TransferRow::into_transfer))
    }

    async fn transition(
        &self,
        transfer_id: Uuid,
        expected_version: i32,
        new_status: CctpTransferStatus,
        patch: TransferPatch,
    ) -> Result<CctpTransfer, CctpStoreError> {
        let current = self.get(transfer_id).await?;
        let Some(current) = current else {
            return Err(CctpStoreError::NotFound);
        };
        if current.version != expected_version {
            return Err(CctpStoreError::VersionConflict);
        }
        if !is_allowed_transition(current.status, new_status) {
            return Err(CctpStoreError::InvalidTransition);
        }

        let terminal = is_terminal(new_status);
        let retry_count = if patch.increment_retry {
            current.retry_count + 1
        } else {
            current.retry_count
        };

        let result = sqlx::query(
            r#"
            UPDATE cctp_transfers SET
                status = $2,
                source_tx_hash = COALESCE($3, source_tx_hash),
                destination_tx_hash = COALESCE($4, destination_tx_hash),
                iris_message_hash = COALESCE($5, iris_message_hash),
                message_nonce = COALESCE($6, message_nonce),
                raw_message = COALESCE($7, raw_message),
                attestation = COALESCE($8, attestation),
                runtime_fee_quote = COALESCE($9, runtime_fee_quote),
                max_fee = COALESCE($10, max_fee),
                fee_expires_at = COALESCE($11, fee_expires_at),
                last_provider_error = COALESCE($12, last_provider_error),
                last_provider_code = COALESCE($13, last_provider_code),
                retry_count = $14,
                version = version + 1,
                updated_at = NOW(),
                terminal_at = CASE WHEN $15 THEN NOW() ELSE terminal_at END
            WHERE transfer_id = $1 AND version = $16
            "#,
        )
        .bind(transfer_id)
        .bind(status_str(new_status))
        .bind(&patch.source_tx_hash)
        .bind(&patch.destination_tx_hash)
        .bind(&patch.iris_message_hash)
        .bind(&patch.message_nonce)
        .bind(&patch.raw_message)
        .bind(&patch.attestation)
        .bind(&patch.runtime_fee_quote)
        .bind(&patch.max_fee)
        .bind(patch.fee_expires_at)
        .bind(&patch.last_provider_error)
        .bind(&patch.last_provider_code)
        .bind(retry_count as i32)
        .bind(terminal)
        .bind(expected_version)
        .execute(&self.pool)
        .await;

        match result {
            Ok(r) if r.rows_affected() == 0 => Err(CctpStoreError::VersionConflict),
            Ok(_) => self.get(transfer_id).await?.ok_or(CctpStoreError::NotFound),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("idx_cctp_source_tx_hash_unique") {
                    Err(CctpStoreError::DuplicateSourceTxHash)
                } else {
                    Err(CctpStoreError::Database(e))
                }
            }
        }
    }
}

#[derive(Default)]
pub struct InMemoryCctpTransferStore {
    transfers: Mutex<HashMap<Uuid, CctpTransfer>>,
    source_tx_hashes: Mutex<HashMap<String, Uuid>>,
}

#[async_trait]
impl CctpTransferStore for InMemoryCctpTransferStore {
    async fn insert(&self, transfer: &CctpTransfer) -> Result<(), CctpStoreError> {
        let mut guard = self.transfers.lock().unwrap();
        if guard.contains_key(&transfer.transfer_id) {
            return Err(CctpStoreError::Database(sqlx::Error::RowNotFound));
        }
        guard.insert(transfer.transfer_id, transfer.clone());
        Ok(())
    }

    async fn get(&self, transfer_id: Uuid) -> Result<Option<CctpTransfer>, CctpStoreError> {
        Ok(self.transfers.lock().unwrap().get(&transfer_id).cloned())
    }

    async fn transition(
        &self,
        transfer_id: Uuid,
        expected_version: i32,
        new_status: CctpTransferStatus,
        patch: TransferPatch,
    ) -> Result<CctpTransfer, CctpStoreError> {
        let mut guard = self.transfers.lock().unwrap();
        let transfer = guard
            .get_mut(&transfer_id)
            .ok_or(CctpStoreError::NotFound)?;
        if transfer.version != expected_version {
            return Err(CctpStoreError::VersionConflict);
        }
        if !is_allowed_transition(transfer.status, new_status) {
            return Err(CctpStoreError::InvalidTransition);
        }

        if let Some(hash) = &patch.source_tx_hash {
            let mut hashes = self.source_tx_hashes.lock().unwrap();
            if let Some(existing) = hashes.get(hash) {
                if *existing != transfer_id {
                    return Err(CctpStoreError::DuplicateSourceTxHash);
                }
            } else {
                hashes.insert(hash.clone(), transfer_id);
            }
            transfer.source_tx_hash = Some(hash.clone());
        }
        if let Some(v) = patch.destination_tx_hash {
            transfer.destination_tx_hash = Some(v);
        }
        if let Some(v) = patch.iris_message_hash {
            transfer.iris_message_hash = Some(v);
        }
        if let Some(v) = patch.message_nonce {
            transfer.message_nonce = Some(v);
        }
        if let Some(v) = patch.raw_message {
            transfer.raw_message = Some(v);
        }
        if let Some(v) = patch.attestation {
            transfer.attestation = Some(v);
        }
        if let Some(v) = patch.runtime_fee_quote {
            transfer.runtime_fee_quote = Some(v);
        }
        if let Some(v) = patch.max_fee {
            transfer.max_fee = Some(v);
        }
        if let Some(v) = patch.fee_expires_at {
            transfer.fee_expires_at = Some(v);
        }
        if let Some(v) = patch.last_provider_error {
            transfer.last_provider_error = Some(v);
        }
        if let Some(v) = patch.last_provider_code {
            transfer.last_provider_code = Some(v);
        }
        if patch.increment_retry {
            transfer.retry_count += 1;
        }

        transfer.status = new_status;
        transfer.version += 1;
        transfer.updated_at = Utc::now();
        if is_terminal(new_status) {
            transfer.terminal_at = Some(Utc::now());
        }
        Ok(transfer.clone())
    }
}

#[derive(sqlx::FromRow)]
struct TransferRow {
    transfer_id: Uuid,
    support_reference_id: String,
    corridor_id: String,
    provider: String,
    direction: String,
    source_chain_id: String,
    destination_chain_id: String,
    source_asset: String,
    source_asset_canonical: String,
    destination_asset: String,
    destination_asset_canonical: String,
    sender: String,
    recipient: String,
    amount: String,
    destination_amount: String,
    finality: String,
    runtime_fee_quote: Option<String>,
    max_fee: Option<String>,
    fee_expires_at: Option<DateTime<Utc>>,
    quote_expires_at: DateTime<Utc>,
    status: String,
    source_tx_hash: Option<String>,
    destination_tx_hash: Option<String>,
    iris_message_hash: Option<String>,
    message_nonce: Option<String>,
    raw_message: Option<Vec<u8>>,
    attestation: Option<Vec<u8>>,
    retry_count: i32,
    last_provider_error: Option<String>,
    last_provider_code: Option<String>,
    version: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    terminal_at: Option<DateTime<Utc>>,
}

impl TransferRow {
    fn into_transfer(self) -> CctpTransfer {
        CctpTransfer {
            transfer_id: self.transfer_id,
            support_reference_id: self.support_reference_id,
            corridor_id: self.corridor_id,
            provider: self.provider,
            direction: parse_direction(&self.direction),
            source_chain_id: self.source_chain_id,
            destination_chain_id: self.destination_chain_id,
            source_asset: self.source_asset,
            source_asset_canonical: self.source_asset_canonical,
            destination_asset: self.destination_asset,
            destination_asset_canonical: self.destination_asset_canonical,
            sender: self.sender,
            recipient: self.recipient,
            amount: self.amount,
            destination_amount: self.destination_amount,
            finality: parse_finality(&self.finality),
            runtime_fee_quote: self.runtime_fee_quote,
            max_fee: self.max_fee,
            fee_expires_at: self.fee_expires_at,
            quote_expires_at: self.quote_expires_at,
            status: parse_status(&self.status),
            source_tx_hash: self.source_tx_hash,
            destination_tx_hash: self.destination_tx_hash,
            iris_message_hash: self.iris_message_hash,
            message_nonce: self.message_nonce,
            raw_message: self.raw_message,
            attestation: self.attestation,
            retry_count: self.retry_count as u32,
            last_provider_error: self.last_provider_error,
            last_provider_code: self.last_provider_code,
            version: self.version,
            created_at: self.created_at,
            updated_at: self.updated_at,
            terminal_at: self.terminal_at,
        }
    }
}

fn direction_str(d: CctpDirection) -> &'static str {
    match d {
        CctpDirection::StellarToEvm => "stellar_to_evm",
        CctpDirection::EvmToStellar => "evm_to_stellar",
    }
}

fn finality_str(f: CctpFinality) -> &'static str {
    match f {
        CctpFinality::Standard => "standard",
        CctpFinality::Fast => "fast",
    }
}

fn status_str(s: CctpTransferStatus) -> &'static str {
    match s {
        CctpTransferStatus::Created => "created",
        CctpTransferStatus::BurnPrepared => "burn_prepared",
        CctpTransferStatus::BurnSubmitted => "burn_submitted",
        CctpTransferStatus::AwaitingAttestation => "awaiting_attestation",
        CctpTransferStatus::AttestationReady => "attestation_ready",
        CctpTransferStatus::MintPrepared => "mint_prepared",
        CctpTransferStatus::MintSubmitted => "mint_submitted",
        CctpTransferStatus::Completed => "completed",
        CctpTransferStatus::AttestationFailed => "attestation_failed",
        CctpTransferStatus::MintFailedRetryable => "mint_failed_retryable",
        CctpTransferStatus::Cancelled => "cancelled",
        CctpTransferStatus::ProviderKilled => "provider_killed",
    }
}

fn parse_direction(s: &str) -> CctpDirection {
    match s {
        "evm_to_stellar" => CctpDirection::EvmToStellar,
        _ => CctpDirection::StellarToEvm,
    }
}

fn parse_finality(s: &str) -> CctpFinality {
    match s {
        "fast" => CctpFinality::Fast,
        _ => CctpFinality::Standard,
    }
}

fn parse_status(s: &str) -> CctpTransferStatus {
    match s {
        "burn_prepared" => CctpTransferStatus::BurnPrepared,
        "burn_submitted" => CctpTransferStatus::BurnSubmitted,
        "awaiting_attestation" => CctpTransferStatus::AwaitingAttestation,
        "attestation_ready" => CctpTransferStatus::AttestationReady,
        "mint_prepared" => CctpTransferStatus::MintPrepared,
        "mint_submitted" => CctpTransferStatus::MintSubmitted,
        "completed" => CctpTransferStatus::Completed,
        "attestation_failed" => CctpTransferStatus::AttestationFailed,
        "mint_failed_retryable" => CctpTransferStatus::MintFailedRetryable,
        "cancelled" => CctpTransferStatus::Cancelled,
        "provider_killed" => CctpTransferStatus::ProviderKilled,
        _ => CctpTransferStatus::Created,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn sample_transfer() -> CctpTransfer {
        let now = Utc::now();
        CctpTransfer {
            transfer_id: Uuid::new_v4(),
            support_reference_id: "sup-1".into(),
            corridor_id: "c".into(),
            provider: "circle-cctp".into(),
            direction: CctpDirection::StellarToEvm,
            source_chain_id: "stellar:testnet".into(),
            destination_chain_id: "eip155:11155111".into(),
            source_asset: "a".into(),
            source_asset_canonical: "a".into(),
            destination_asset: "b".into(),
            destination_asset_canonical: "b".into(),
            sender: "".into(),
            recipient: "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0".into(),
            amount: "10".into(),
            destination_amount: "10".into(),
            finality: CctpFinality::Standard,
            runtime_fee_quote: None,
            max_fee: None,
            fee_expires_at: None,
            quote_expires_at: now + Duration::minutes(5),
            status: CctpTransferStatus::Created,
            source_tx_hash: None,
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
        }
    }

    #[tokio::test]
    async fn in_memory_transition_happy_path() {
        let store = InMemoryCctpTransferStore::default();
        let t = sample_transfer();
        let id = t.transfer_id;
        store.insert(&t).await.unwrap();

        let updated = store
            .transition(
                id,
                1,
                CctpTransferStatus::BurnPrepared,
                TransferPatch::default(),
            )
            .await
            .unwrap();
        assert_eq!(updated.status, CctpTransferStatus::BurnPrepared);
        assert_eq!(updated.version, 2);
    }

    #[tokio::test]
    async fn in_memory_rejects_invalid_transition() {
        let store = InMemoryCctpTransferStore::default();
        let t = sample_transfer();
        let id = t.transfer_id;
        store.insert(&t).await.unwrap();
        let err = store
            .transition(
                id,
                1,
                CctpTransferStatus::Completed,
                TransferPatch::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, CctpStoreError::InvalidTransition));
    }

    #[tokio::test]
    async fn duplicate_source_tx_hash_rejected() {
        let store = InMemoryCctpTransferStore::default();
        let t1 = sample_transfer();
        let t2 = sample_transfer();
        let id1 = t1.transfer_id;
        let id2 = t2.transfer_id;
        store.insert(&t1).await.unwrap();
        store.insert(&t2).await.unwrap();
        store
            .transition(
                id1,
                1,
                CctpTransferStatus::BurnPrepared,
                TransferPatch::default(),
            )
            .await
            .unwrap();
        store
            .transition(
                id1,
                2,
                CctpTransferStatus::BurnSubmitted,
                TransferPatch {
                    source_tx_hash: Some("hash1".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        store
            .transition(
                id2,
                1,
                CctpTransferStatus::BurnPrepared,
                TransferPatch::default(),
            )
            .await
            .unwrap();
        let err = store
            .transition(
                id2,
                2,
                CctpTransferStatus::BurnSubmitted,
                TransferPatch {
                    source_tx_hash: Some("hash1".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, CctpStoreError::DuplicateSourceTxHash));
    }
}
