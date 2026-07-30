//! Durable quote idempotency for `POST /api/v2/bridge/cctp/quote`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;
use uuid::Uuid;

use crate::cctp::bounds::check_str_len;

pub const IDEMPOTENCY_HEADER: &str = "idempotency-key";
pub const MAX_IDEMPOTENCY_KEY_LEN: usize = 128;
pub const MAX_QUOTE_REQUEST_BYTES: usize = 8_192;

#[derive(Debug, Error)]
pub enum CctpIdempotencyError {
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("key too long")]
    KeyTooLong,
    #[error("request too large")]
    RequestTooLarge,
    #[error("conflict: idempotency key reused with different request")]
    Conflict,
}

#[derive(Debug, Clone)]
pub struct CctpIdempotencyRecord {
    pub transfer_id: Uuid,
    pub response_json: String,
}

#[async_trait]
pub trait CctpQuoteIdempotencyStore: Send + Sync {
    async fn lookup(
        &self,
        key: &str,
    ) -> Result<Option<(String, CctpIdempotencyRecord)>, CctpIdempotencyError>;

    async fn insert(
        &self,
        key: &str,
        request_hash: &str,
        transfer_id: Uuid,
        response_json: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), CctpIdempotencyError>;
}

pub fn hash_quote_request(body: &[u8]) -> Result<String, CctpIdempotencyError> {
    if body.len() > MAX_QUOTE_REQUEST_BYTES {
        return Err(CctpIdempotencyError::RequestTooLarge);
    }
    Ok(hex::encode(Sha256::digest(body)))
}

pub fn normalize_idempotency_key(key: &str) -> Result<String, CctpIdempotencyError> {
    let trimmed = key.trim();
    check_str_len("idempotency_key", trimmed, MAX_IDEMPOTENCY_KEY_LEN)
        .map_err(|_| CctpIdempotencyError::KeyTooLong)?;
    if trimmed.is_empty() {
        return Err(CctpIdempotencyError::KeyTooLong);
    }
    Ok(trimmed.to_string())
}

pub struct PgCctpQuoteIdempotencyStore {
    pool: PgPool,
}

impl PgCctpQuoteIdempotencyStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CctpQuoteIdempotencyStore for PgCctpQuoteIdempotencyStore {
    async fn lookup(
        &self,
        key: &str,
    ) -> Result<Option<(String, CctpIdempotencyRecord)>, CctpIdempotencyError> {
        let row = sqlx::query_as::<_, (String, Uuid, String)>(
            r#"
            SELECT request_hash, transfer_id, response_json
            FROM cctp_quote_idempotency
            WHERE idempotency_key = $1 AND expires_at > NOW()
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(request_hash, transfer_id, response_json)| {
            (
                request_hash,
                CctpIdempotencyRecord {
                    transfer_id,
                    response_json,
                },
            )
        }))
    }

    async fn insert(
        &self,
        key: &str,
        request_hash: &str,
        transfer_id: Uuid,
        response_json: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), CctpIdempotencyError> {
        sqlx::query(
            r#"
            INSERT INTO cctp_quote_idempotency
                (idempotency_key, request_hash, transfer_id, response_json, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (idempotency_key) DO NOTHING
            "#,
        )
        .bind(key)
        .bind(request_hash)
        .bind(transfer_id)
        .bind(response_json)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryCctpQuoteIdempotencyStore {
    entries: Mutex<IdempotencyMap>,
}

type IdempotencyMap = HashMap<String, (String, CctpIdempotencyRecord, DateTime<Utc>)>;

#[async_trait]
impl CctpQuoteIdempotencyStore for InMemoryCctpQuoteIdempotencyStore {
    async fn lookup(
        &self,
        key: &str,
    ) -> Result<Option<(String, CctpIdempotencyRecord)>, CctpIdempotencyError> {
        let guard = self.entries.lock().unwrap();
        let Some((hash, record, expires)) = guard.get(key) else {
            return Ok(None);
        };
        if *expires <= Utc::now() {
            return Ok(None);
        }
        Ok(Some((hash.clone(), record.clone())))
    }

    async fn insert(
        &self,
        key: &str,
        request_hash: &str,
        transfer_id: Uuid,
        response_json: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), CctpIdempotencyError> {
        let mut guard = self.entries.lock().unwrap();
        if guard.contains_key(key) {
            return Ok(());
        }
        guard.insert(
            key.to_string(),
            (
                request_hash.to_string(),
                CctpIdempotencyRecord {
                    transfer_id,
                    response_json: response_json.to_string(),
                },
                expires_at,
            ),
        );
        Ok(())
    }
}
