//! Destination attester-set snapshots with atomic cache swap.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::cctp::attestation_crypto::keccak256;
use crate::cctp::config::{CctpConfig, SEPOLIA_DOMAIN, STELLAR_TESTNET_DOMAIN};
use crate::cctp::iris_public_keys::IrisPublicKeyCache;
use crate::metrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttesterDestination {
    Sepolia,
    StellarTestnet,
}

impl AttesterDestination {
    pub fn domain(self) -> u32 {
        match self {
            Self::Sepolia => SEPOLIA_DOMAIN,
            Self::StellarTestnet => STELLAR_TESTNET_DOMAIN,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sepolia => "sepolia",
            Self::StellarTestnet => "stellar_testnet",
        }
    }
}

#[derive(Clone)]
pub struct AttesterSetSnapshot {
    pub destination: AttesterDestination,
    pub signature_threshold: u32,
    pub enabled_addresses: Vec<[u8; 20]>,
    pub iris_set_hash: [u8; 32],
    pub verified_at: Instant,
    pub block_or_ledger: Option<String>,
    pub source: &'static str,
}

impl AttesterSetSnapshot {
    pub fn is_fresh(&self, ttl: Duration) -> bool {
        self.verified_at.elapsed() <= ttl
    }

    pub fn is_stale_beyond(&self, max_stale: Duration) -> bool {
        self.verified_at.elapsed() > max_stale
    }

    pub fn set_hash(addresses: &[[u8; 20]]) -> [u8; 32] {
        let mut sorted = addresses.to_vec();
        sorted.sort();
        let mut hasher = Sha256::new();
        for addr in sorted {
            hasher.update(addr);
        }
        let digest = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        out
    }
}

impl fmt::Debug for AttesterSetSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttesterSetSnapshot")
            .field("destination", &self.destination)
            .field("signature_threshold", &self.signature_threshold)
            .field("enabled_count", &self.enabled_addresses.len())
            .field("iris_set_hash", &hex::encode(self.iris_set_hash))
            .field("block_or_ledger", &self.block_or_ledger)
            .field("source", &self.source)
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum AttesterSetError {
    #[error("not ready")]
    NotReady,
    #[error("transient: {0}")]
    Transient(String),
    #[error("threshold zero")]
    ThresholdZero,
    #[error("insufficient enabled attesters")]
    InsufficientEnabled,
    #[error("on-chain not in iris candidate set")]
    OnChainNotInIris,
    #[error("empty enabled set")]
    EmptySet,
    #[error("stale snapshot")]
    Stale,
}

#[async_trait]
pub trait AttesterSetReader: Send + Sync {
    fn destination(&self) -> AttesterDestination;
    async fn read_snapshot(
        &self,
        iris_candidates: &[[u8; 20]],
        iris_set_hash: [u8; 32],
    ) -> Result<AttesterSetSnapshot, AttesterSetError>;
}

pub struct AttesterSetCache {
    sepolia: ArcSwap<Option<AttesterSetSnapshot>>,
    stellar: ArcSwap<Option<AttesterSetSnapshot>>,
    pub ttl: Duration,
    pub stale_max: Duration,
    refresh_lock: Mutex<()>,
}

impl AttesterSetCache {
    pub fn new(ttl: Duration, stale_max: Duration) -> Self {
        Self {
            sepolia: ArcSwap::from_pointee(None),
            stellar: ArcSwap::from_pointee(None),
            ttl,
            stale_max,
            refresh_lock: Mutex::new(()),
        }
    }

    pub fn from_config(config: &CctpConfig) -> Self {
        Self::new(
            Duration::from_secs(config.attester_snapshot_ttl_secs),
            Duration::from_secs(config.attester_snapshot_stale_max_secs),
        )
    }

    fn slot(&self, dest: AttesterDestination) -> &ArcSwap<Option<AttesterSetSnapshot>> {
        match dest {
            AttesterDestination::Sepolia => &self.sepolia,
            AttesterDestination::StellarTestnet => &self.stellar,
        }
    }

    pub fn get(&self, dest: AttesterDestination) -> Option<AttesterSetSnapshot> {
        self.slot(dest).load_full().as_ref().clone()
    }

    pub fn is_ready(&self, dest: AttesterDestination) -> bool {
        self.get(dest)
            .map(|s| s.is_fresh(self.ttl) && !s.is_stale_beyond(self.stale_max))
            .unwrap_or(false)
    }

    pub fn is_bidirectional_ready(&self) -> bool {
        self.is_ready(AttesterDestination::Sepolia)
            && self.is_ready(AttesterDestination::StellarTestnet)
    }

    pub fn swap(&self, snapshot: AttesterSetSnapshot) {
        metrics::record_cctp_attester_snapshot_refresh(snapshot.destination.label(), "success");
        self.slot(snapshot.destination)
            .store(Arc::new(Some(snapshot)));
    }

    pub async fn refresh_destination(
        &self,
        reader: &dyn AttesterSetReader,
        iris: &IrisPublicKeyCache,
    ) -> Result<(), AttesterSetError> {
        let _guard = self.refresh_lock.lock().await;
        let candidates = iris.snapshot().ok_or(AttesterSetError::NotReady)?;
        if candidates.addresses.is_empty() {
            return Err(AttesterSetError::EmptySet);
        }
        let snapshot = reader
            .read_snapshot(&candidates.addresses, candidates.set_hash)
            .await?;
        self.swap(snapshot);
        Ok(())
    }

    pub async fn refresh_all(
        &self,
        readers: &[Arc<dyn AttesterSetReader>],
        iris: &IrisPublicKeyCache,
    ) -> Result<(), AttesterSetError> {
        for reader in readers {
            if let Err(e) = self.refresh_destination(reader.as_ref(), iris).await {
                metrics::record_cctp_attester_snapshot_refresh(
                    reader.destination().label(),
                    "failure",
                );
                return Err(e);
            }
        }
        Ok(())
    }
}

pub fn destination_for_message(
    source_domain: u32,
    destination_domain: u32,
) -> Option<AttesterDestination> {
    if source_domain == STELLAR_TESTNET_DOMAIN && destination_domain == SEPOLIA_DOMAIN {
        Some(AttesterDestination::Sepolia)
    } else if source_domain == SEPOLIA_DOMAIN && destination_domain == STELLAR_TESTNET_DOMAIN {
        Some(AttesterDestination::StellarTestnet)
    } else {
        None
    }
}

pub fn iris_candidate_hash(addresses: &[[u8; 20]]) -> [u8; 32] {
    keccak256(
        &addresses
            .iter()
            .flat_map(|a| a.iter().copied())
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_mapping_for_corridor() {
        assert_eq!(
            destination_for_message(27, 0),
            Some(AttesterDestination::Sepolia)
        );
        assert_eq!(
            destination_for_message(0, 27),
            Some(AttesterDestination::StellarTestnet)
        );
        assert_eq!(destination_for_message(1, 2), None);
    }

    #[test]
    fn snapshot_freshness() {
        let snap = AttesterSetSnapshot {
            destination: AttesterDestination::Sepolia,
            signature_threshold: 2,
            enabled_addresses: vec![],
            iris_set_hash: [0u8; 32],
            verified_at: Instant::now(),
            block_or_ledger: None,
            source: "test",
        };
        assert!(snap.is_fresh(Duration::from_secs(60)));
        assert!(!snap.is_stale_beyond(Duration::from_secs(60)));
    }
}
