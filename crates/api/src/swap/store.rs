//! Swap prepare/submit persistence and idempotency.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;

/// A prepared swap quote awaiting client signature and submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSwapQuote {
    pub quote_id: String,
    pub sender_account_hash: String,
    pub unsigned_xdr_hash: String,
    pub expires_at: DateTime<Utc>,
    pub estimated_output: String,
    pub min_output: String,
    pub valid_until_ledger: Option<i64>,
    pub submission_status: SubmissionStatus,
    pub tx_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionStatus {
    Prepared,
    Submitting,
    Submitted,
    Failed,
}

impl SubmissionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Submitting => "submitting",
            Self::Submitted => "submitted",
            Self::Failed => "failed",
        }
    }

    fn from_db(value: &str) -> Option<Self> {
        match value {
            "prepared" => Some(Self::Prepared),
            "submitting" => Some(Self::Submitting),
            "submitted" => Some(Self::Submitted),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum SwapStoreError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("quote not found")]
    NotFound,
}

/// Result of attempting to claim a quote for broadcast (prevents double-submit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimSubmitOutcome {
    /// Quote is locked for this submit attempt; caller must broadcast then finalize.
    Claimed(PreparedSwapQuote),
    /// Another submit is in flight for this quote_id.
    InProgress,
    /// Quote was already broadcast successfully.
    AlreadySubmitted { tx_hash: String },
}

#[async_trait]
pub trait SwapQuoteStore: Send + Sync {
    async fn insert_prepared(&self, quote: &PreparedSwapQuote) -> Result<(), SwapStoreError>;

    async fn get(&self, quote_id: &str) -> Result<Option<PreparedSwapQuote>, SwapStoreError>;

    async fn claim_for_submit(&self, quote_id: &str) -> Result<ClaimSubmitOutcome, SwapStoreError>;

    async fn finalize_submit(
        &self,
        quote_id: &str,
        tx_hash: &str,
    ) -> Result<(), SwapStoreError>;

    async fn mark_failed(&self, quote_id: &str) -> Result<(), SwapStoreError>;
}

/// Postgres-backed store (production).
#[derive(Clone)]
pub struct PgSwapQuoteStore {
    pool: PgPool,
}

impl PgSwapQuoteStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SwapQuoteStore for PgSwapQuoteStore {
    async fn insert_prepared(&self, quote: &PreparedSwapQuote) -> Result<(), SwapStoreError> {
        sqlx::query(
            r#"
            INSERT INTO swap_prepared_quotes (
                quote_id, sender_account_hash, unsigned_xdr_hash, expires_at,
                estimated_output, min_output, valid_until_ledger, submission_status
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(&quote.quote_id)
        .bind(&quote.sender_account_hash)
        .bind(&quote.unsigned_xdr_hash)
        .bind(quote.expires_at)
        .bind(&quote.estimated_output)
        .bind(&quote.min_output)
        .bind(quote.valid_until_ledger)
        .bind(quote.submission_status.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, quote_id: &str) -> Result<Option<PreparedSwapQuote>, SwapStoreError> {
        let row = sqlx::query_as::<_, PreparedQuoteRow>(
            r#"
            SELECT quote_id, sender_account_hash, unsigned_xdr_hash, expires_at,
                   estimated_output, min_output, valid_until_ledger,
                   submission_status, tx_hash
            FROM swap_prepared_quotes
            WHERE quote_id = $1
            "#,
        )
        .bind(quote_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(PreparedQuoteRow::into_quote))
    }

    async fn claim_for_submit(&self, quote_id: &str) -> Result<ClaimSubmitOutcome, SwapStoreError> {
        let mut tx = self.pool.begin().await?;

        let existing = sqlx::query_as::<_, PreparedQuoteRow>(
            r#"
            SELECT quote_id, sender_account_hash, unsigned_xdr_hash, expires_at,
                   estimated_output, min_output, valid_until_ledger,
                   submission_status, tx_hash
            FROM swap_prepared_quotes
            WHERE quote_id = $1
            FOR UPDATE
            "#,
        )
        .bind(quote_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = existing else {
            return Err(SwapStoreError::NotFound);
        };

        let status = SubmissionStatus::from_db(&row.submission_status).unwrap_or(SubmissionStatus::Failed);

        match status {
            SubmissionStatus::Submitted => {
                tx.commit().await?;
                return Ok(ClaimSubmitOutcome::AlreadySubmitted {
                    tx_hash: row.tx_hash.unwrap_or_default(),
                });
            }
            SubmissionStatus::Submitting => {
                tx.commit().await?;
                return Ok(ClaimSubmitOutcome::InProgress);
            }
            SubmissionStatus::Failed => {
                tx.commit().await?;
                return Err(SwapStoreError::NotFound);
            }
            SubmissionStatus::Prepared => {
                sqlx::query(
                    r#"
                    UPDATE swap_prepared_quotes
                    SET submission_status = 'submitting'
                    WHERE quote_id = $1 AND submission_status = 'prepared'
                    "#,
                )
                .bind(quote_id)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                Ok(ClaimSubmitOutcome::Claimed(row.into_quote()))
            }
        }
    }

    async fn finalize_submit(&self, quote_id: &str, tx_hash: &str) -> Result<(), SwapStoreError> {
        sqlx::query(
            r#"
            UPDATE swap_prepared_quotes
            SET submission_status = 'submitted', tx_hash = $2, submitted_at = NOW()
            WHERE quote_id = $1 AND submission_status = 'submitting'
            "#,
        )
        .bind(quote_id)
        .bind(tx_hash)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn mark_failed(&self, quote_id: &str) -> Result<(), SwapStoreError> {
        sqlx::query(
            r#"
            UPDATE swap_prepared_quotes
            SET submission_status = 'failed'
            WHERE quote_id = $1 AND submission_status IN ('prepared', 'submitting')
            "#,
        )
        .bind(quote_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct PreparedQuoteRow {
    quote_id: String,
    sender_account_hash: String,
    unsigned_xdr_hash: String,
    expires_at: DateTime<Utc>,
    estimated_output: String,
    min_output: String,
    valid_until_ledger: Option<i64>,
    submission_status: String,
    tx_hash: Option<String>,
}

impl PreparedQuoteRow {
    fn into_quote(self) -> PreparedSwapQuote {
        PreparedSwapQuote {
            quote_id: self.quote_id,
            sender_account_hash: self.sender_account_hash,
            unsigned_xdr_hash: self.unsigned_xdr_hash,
            expires_at: self.expires_at,
            estimated_output: self.estimated_output,
            min_output: self.min_output,
            valid_until_ledger: self.valid_until_ledger,
            submission_status: SubmissionStatus::from_db(&self.submission_status)
                .unwrap_or(SubmissionStatus::Failed),
            tx_hash: self.tx_hash,
        }
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct InMemorySwapQuoteStore {
    quotes: Mutex<HashMap<String, PreparedSwapQuote>>,
}

#[async_trait]
impl SwapQuoteStore for InMemorySwapQuoteStore {
    async fn insert_prepared(&self, quote: &PreparedSwapQuote) -> Result<(), SwapStoreError> {
        self.quotes
            .lock()
            .unwrap()
            .insert(quote.quote_id.clone(), quote.clone());
        Ok(())
    }

    async fn get(&self, quote_id: &str) -> Result<Option<PreparedSwapQuote>, SwapStoreError> {
        Ok(self.quotes.lock().unwrap().get(quote_id).cloned())
    }

    async fn claim_for_submit(&self, quote_id: &str) -> Result<ClaimSubmitOutcome, SwapStoreError> {
        let mut guard = self.quotes.lock().unwrap();
        let Some(quote) = guard.get_mut(quote_id) else {
            return Err(SwapStoreError::NotFound);
        };
        match quote.submission_status {
            SubmissionStatus::Submitted => Ok(ClaimSubmitOutcome::AlreadySubmitted {
                tx_hash: quote.tx_hash.clone().unwrap_or_default(),
            }),
            SubmissionStatus::Submitting => Ok(ClaimSubmitOutcome::InProgress),
            SubmissionStatus::Failed => Err(SwapStoreError::NotFound),
            SubmissionStatus::Prepared => {
                quote.submission_status = SubmissionStatus::Submitting;
                Ok(ClaimSubmitOutcome::Claimed(quote.clone()))
            }
        }
    }

    async fn finalize_submit(&self, quote_id: &str, tx_hash: &str) -> Result<(), SwapStoreError> {
        let mut guard = self.quotes.lock().unwrap();
        let Some(quote) = guard.get_mut(quote_id) else {
            return Err(SwapStoreError::NotFound);
        };
        quote.submission_status = SubmissionStatus::Submitted;
        quote.tx_hash = Some(tx_hash.to_string());
        Ok(())
    }

    async fn mark_failed(&self, quote_id: &str) -> Result<(), SwapStoreError> {
        let mut guard = self.quotes.lock().unwrap();
        if let Some(quote) = guard.get_mut(quote_id) {
            quote.submission_status = SubmissionStatus::Failed;
        }
        Ok(())
    }
}

pub fn hash_xdr(xdr: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(xdr.as_bytes());
    hex::encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn sample_quote(id: &str) -> PreparedSwapQuote {
        PreparedSwapQuote {
            quote_id: id.to_string(),
            sender_account_hash: "GABC...#abcd".to_string(),
            unsigned_xdr_hash: "unsigned-hash".to_string(),
            expires_at: Utc::now() + Duration::minutes(5),
            estimated_output: "98".to_string(),
            min_output: "97".to_string(),
            valid_until_ledger: Some(123),
            submission_status: SubmissionStatus::Prepared,
            tx_hash: None,
        }
    }

    #[tokio::test]
    async fn in_memory_claim_prevents_double_broadcast() {
        let store = InMemorySwapQuoteStore::default();
        store.insert_prepared(&sample_quote("q1")).await.unwrap();

        let first = store.claim_for_submit("q1").await.unwrap();
        assert!(matches!(first, ClaimSubmitOutcome::Claimed(_)));

        let second = store.claim_for_submit("q1").await.unwrap();
        assert!(matches!(second, ClaimSubmitOutcome::InProgress));

        store.finalize_submit("q1", "hash-1").await.unwrap();

        let third = store.claim_for_submit("q1").await.unwrap();
        assert!(matches!(third, ClaimSubmitOutcome::AlreadySubmitted { .. }));
    }
}
