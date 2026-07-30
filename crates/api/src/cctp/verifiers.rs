//! Source-chain burn verification traits (production parsers deferred).

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBurnFacts {
    pub source_chain_id: String,
    pub sender: String,
    pub recipient_bytes32: [u8; 32],
    pub amount_cctp_subunits: u128,
    pub source_domain: u32,
    pub destination_domain: u32,
    pub burn_token_bytes32: [u8; 32],
    pub min_finality_threshold: u32,
    pub block_or_ledger: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VerifierError {
    #[error("not ready")]
    NotReady,
    #[error("tx not found")]
    TxNotFound,
    #[error("verification failed: {0}")]
    Failed(String),
}

#[async_trait]
pub trait StellarBurnVerifier: Send + Sync {
    async fn verify_burn(&self, tx_hash: &str) -> Result<VerifiedBurnFacts, VerifierError>;
}

#[async_trait]
pub trait EvmBurnVerifier: Send + Sync {
    async fn verify_burn(&self, tx_hash: &str) -> Result<VerifiedBurnFacts, VerifierError>;
}

/// Production placeholder — event parsing lands with transaction-builder task.
pub struct NotReadyStellarBurnVerifier;

#[async_trait]
impl StellarBurnVerifier for NotReadyStellarBurnVerifier {
    async fn verify_burn(&self, _tx_hash: &str) -> Result<VerifiedBurnFacts, VerifierError> {
        Err(VerifierError::NotReady)
    }
}

pub struct NotReadyEvmBurnVerifier;

#[async_trait]
impl EvmBurnVerifier for NotReadyEvmBurnVerifier {
    async fn verify_burn(&self, _tx_hash: &str) -> Result<VerifiedBurnFacts, VerifierError> {
        Err(VerifierError::NotReady)
    }
}

/// Deterministic fake for service tests.
pub struct FakeBurnVerifier {
    pub facts: VerifiedBurnFacts,
}

#[async_trait]
impl StellarBurnVerifier for FakeBurnVerifier {
    async fn verify_burn(&self, _tx_hash: &str) -> Result<VerifiedBurnFacts, VerifierError> {
        Ok(self.facts.clone())
    }
}

#[async_trait]
impl EvmBurnVerifier for FakeBurnVerifier {
    async fn verify_burn(&self, _tx_hash: &str) -> Result<VerifiedBurnFacts, VerifierError> {
        Ok(self.facts.clone())
    }
}
