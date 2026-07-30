//! Per-Stellar-source active unsigned prepare reservation (multi-instance safe).
//!
//! Mirrors swap `ActivePrepareExists` semantics: at most one live unsigned prepare
//! per source account across API replicas.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CctpPrepareKind {
    Approval,
    Burn,
    Mint,
}

impl CctpPrepareKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approval => "approval",
            Self::Burn => "burn",
            Self::Mint => "mint",
        }
    }

    pub fn parse_kind(s: &str) -> Option<Self> {
        match s {
            "approval" => Some(Self::Approval),
            "burn" => Some(Self::Burn),
            "mint" => Some(Self::Mint),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CctpActivePrepare {
    pub source_account: String,
    pub transfer_id: Uuid,
    pub kind: CctpPrepareKind,
    pub payload_hash: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CctpPrepareLockError {
    #[error("active prepare already exists for source")]
    ActivePrepareExists,
    #[error("database: {0}")]
    Database(String),
}

#[async_trait]
pub trait CctpPrepareLockStore: Send + Sync {
    async fn expire_stale_for_source(
        &self,
        source_account: &str,
    ) -> Result<u64, CctpPrepareLockError>;

    async fn try_acquire(
        &self,
        reservation: &CctpActivePrepare,
    ) -> Result<(), CctpPrepareLockError>;

    async fn release(
        &self,
        source_account: &str,
        transfer_id: Uuid,
    ) -> Result<(), CctpPrepareLockError>;

    async fn get_active(
        &self,
        source_account: &str,
    ) -> Result<Option<CctpActivePrepare>, CctpPrepareLockError>;
}

#[derive(Default)]
pub struct InMemoryCctpPrepareLockStore {
    locks: Mutex<HashMap<String, CctpActivePrepare>>,
}

#[async_trait]
impl CctpPrepareLockStore for InMemoryCctpPrepareLockStore {
    async fn expire_stale_for_source(
        &self,
        source_account: &str,
    ) -> Result<u64, CctpPrepareLockError> {
        let mut guard = self.locks.lock().unwrap();
        let now = Utc::now();
        let mut removed = 0u64;
        guard.retain(|k, v| {
            if k == source_account && v.expires_at <= now {
                removed += 1;
                false
            } else {
                true
            }
        });
        Ok(removed)
    }

    async fn try_acquire(
        &self,
        reservation: &CctpActivePrepare,
    ) -> Result<(), CctpPrepareLockError> {
        self.expire_stale_for_source(&reservation.source_account)
            .await?;
        let mut guard = self.locks.lock().unwrap();
        if let Some(existing) = guard.get(&reservation.source_account) {
            if existing.expires_at > Utc::now() && existing.transfer_id != reservation.transfer_id {
                return Err(CctpPrepareLockError::ActivePrepareExists);
            }
        }
        guard.insert(reservation.source_account.clone(), reservation.clone());
        Ok(())
    }

    async fn release(
        &self,
        source_account: &str,
        transfer_id: Uuid,
    ) -> Result<(), CctpPrepareLockError> {
        let mut guard = self.locks.lock().unwrap();
        if let Some(existing) = guard.get(source_account) {
            if existing.transfer_id == transfer_id {
                guard.remove(source_account);
            }
        }
        Ok(())
    }

    async fn get_active(
        &self,
        source_account: &str,
    ) -> Result<Option<CctpActivePrepare>, CctpPrepareLockError> {
        self.expire_stale_for_source(source_account).await?;
        Ok(self.locks.lock().unwrap().get(source_account).cloned())
    }
}

pub struct PgCctpPrepareLockStore {
    pool: PgPool,
}

impl PgCctpPrepareLockStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CctpPrepareLockStore for PgCctpPrepareLockStore {
    async fn expire_stale_for_source(
        &self,
        source_account: &str,
    ) -> Result<u64, CctpPrepareLockError> {
        let result = sqlx::query(
            r#"DELETE FROM cctp_active_prepares WHERE source_account = $1 AND expires_at <= NOW()"#,
        )
        .bind(source_account)
        .execute(&self.pool)
        .await
        .map_err(|e| CctpPrepareLockError::Database(e.to_string()))?;
        Ok(result.rows_affected())
    }

    async fn try_acquire(
        &self,
        reservation: &CctpActivePrepare,
    ) -> Result<(), CctpPrepareLockError> {
        self.expire_stale_for_source(&reservation.source_account)
            .await?;
        let result = sqlx::query(
            r#"
            INSERT INTO cctp_active_prepares (
                source_account, transfer_id, prepare_kind, payload_hash, expires_at
            ) VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (source_account) DO NOTHING
            "#,
        )
        .bind(&reservation.source_account)
        .bind(reservation.transfer_id)
        .bind(reservation.kind.as_str())
        .bind(&reservation.payload_hash)
        .bind(reservation.expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| CctpPrepareLockError::Database(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(CctpPrepareLockError::ActivePrepareExists);
        }
        Ok(())
    }

    async fn release(
        &self,
        source_account: &str,
        transfer_id: Uuid,
    ) -> Result<(), CctpPrepareLockError> {
        sqlx::query(
            r#"DELETE FROM cctp_active_prepares WHERE source_account = $1 AND transfer_id = $2"#,
        )
        .bind(source_account)
        .bind(transfer_id)
        .execute(&self.pool)
        .await
        .map_err(|e| CctpPrepareLockError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_active(
        &self,
        source_account: &str,
    ) -> Result<Option<CctpActivePrepare>, CctpPrepareLockError> {
        self.expire_stale_for_source(source_account).await?;
        let row = sqlx::query_as::<_, (String, Uuid, String, String, DateTime<Utc>)>(
            r#"
            SELECT source_account, transfer_id, prepare_kind, payload_hash, expires_at
            FROM cctp_active_prepares
            WHERE source_account = $1 AND expires_at > NOW()
            "#,
        )
        .bind(source_account)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CctpPrepareLockError::Database(e.to_string()))?;
        Ok(row.map(
            |(source_account, transfer_id, kind, payload_hash, expires_at)| CctpActivePrepare {
                source_account,
                transfer_id,
                kind: CctpPrepareKind::parse_kind(&kind).unwrap_or(CctpPrepareKind::Burn),
                payload_hash,
                expires_at,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[tokio::test]
    async fn concurrent_prepare_same_source_rejected() {
        let store = InMemoryCctpPrepareLockStore::default();
        let source = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
        let r1 = CctpActivePrepare {
            source_account: source.into(),
            transfer_id: Uuid::new_v4(),
            kind: CctpPrepareKind::Approval,
            payload_hash: "a".into(),
            expires_at: Utc::now() + Duration::minutes(5),
        };
        store.try_acquire(&r1).await.unwrap();
        let r2 = CctpActivePrepare {
            transfer_id: Uuid::new_v4(),
            ..r1.clone()
        };
        assert_eq!(
            store.try_acquire(&r2).await,
            Err(CctpPrepareLockError::ActivePrepareExists)
        );
    }

    #[tokio::test]
    async fn distinct_sources_proceed() {
        let store = InMemoryCctpPrepareLockStore::default();
        let mk = |g: &str| CctpActivePrepare {
            source_account: g.into(),
            transfer_id: Uuid::new_v4(),
            kind: CctpPrepareKind::Burn,
            payload_hash: "b".into(),
            expires_at: Utc::now() + Duration::minutes(5),
        };
        store
            .try_acquire(&mk(
                "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            ))
            .await
            .unwrap();
        store
            .try_acquire(&mk(
                "GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            ))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn expired_reservation_recovers() {
        let store = InMemoryCctpPrepareLockStore::default();
        let source = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
        let stale = CctpActivePrepare {
            source_account: source.into(),
            transfer_id: Uuid::new_v4(),
            kind: CctpPrepareKind::Approval,
            payload_hash: "old".into(),
            expires_at: Utc::now() - Duration::seconds(1),
        };
        store.try_acquire(&stale).await.unwrap();
        let fresh = CctpActivePrepare {
            transfer_id: Uuid::new_v4(),
            payload_hash: "new".into(),
            expires_at: Utc::now() + Duration::minutes(5),
            ..stale
        };
        store.try_acquire(&fresh).await.unwrap();
    }
}
