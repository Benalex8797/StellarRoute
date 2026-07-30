//! Source-chain burn verification traits (production parsers deferred).

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBurnFacts {
    pub tx_hash: String,
    pub source_chain_id: String,
    pub source_domain: u32,
    pub destination_domain: u32,
    pub sender: String,
    pub amount_cctp_subunits: u128,
    pub burn_token_bytes32: [u8; 32],
    pub mint_recipient_bytes32: [u8; 32],
    pub destination_caller_bytes32: [u8; 32],
    pub min_finality_threshold: u32,
    pub hook_data: Option<Vec<u8>>,
    pub token_messenger_bytes32: [u8; 32],
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
    fn is_ready(&self) -> bool;
    async fn verify_burn(&self, tx_hash: &str) -> Result<VerifiedBurnFacts, VerifierError>;
}

#[async_trait]
pub trait EvmBurnVerifier: Send + Sync {
    fn is_ready(&self) -> bool;
    async fn verify_burn(&self, tx_hash: &str) -> Result<VerifiedBurnFacts, VerifierError>;
}

/// Production placeholder — event parsing lands with transaction-builder task.
pub struct NotReadyStellarBurnVerifier;

#[async_trait]
impl StellarBurnVerifier for NotReadyStellarBurnVerifier {
    fn is_ready(&self) -> bool {
        false
    }

    async fn verify_burn(&self, _tx_hash: &str) -> Result<VerifiedBurnFacts, VerifierError> {
        Err(VerifierError::NotReady)
    }
}

pub struct NotReadyEvmBurnVerifier;

#[async_trait]
impl EvmBurnVerifier for NotReadyEvmBurnVerifier {
    fn is_ready(&self) -> bool {
        false
    }

    async fn verify_burn(&self, _tx_hash: &str) -> Result<VerifiedBurnFacts, VerifierError> {
        Err(VerifierError::NotReady)
    }
}

/// Deterministic fake for service tests.
pub struct FakeBurnVerifier {
    pub facts: VerifiedBurnFacts,
    pub ready: bool,
}

#[async_trait]
impl StellarBurnVerifier for FakeBurnVerifier {
    fn is_ready(&self) -> bool {
        self.ready
    }

    async fn verify_burn(&self, tx_hash: &str) -> Result<VerifiedBurnFacts, VerifierError> {
        if !self.ready {
            return Err(VerifierError::NotReady);
        }
        if self.facts.tx_hash != tx_hash {
            return Err(VerifierError::Failed("tx_hash mismatch".into()));
        }
        Ok(self.facts.clone())
    }
}

#[async_trait]
impl EvmBurnVerifier for FakeBurnVerifier {
    fn is_ready(&self) -> bool {
        self.ready
    }

    async fn verify_burn(&self, tx_hash: &str) -> Result<VerifiedBurnFacts, VerifierError> {
        if !self.ready {
            return Err(VerifierError::NotReady);
        }
        if self.facts.tx_hash != tx_hash {
            return Err(VerifierError::Failed("tx_hash mismatch".into()));
        }
        Ok(self.facts.clone())
    }
}

pub fn facts_match(expected: &VerifiedBurnFacts, actual: &VerifiedBurnFacts) -> Result<(), String> {
    if expected.tx_hash != actual.tx_hash {
        return Err("tx_hash".into());
    }
    if expected.source_chain_id != actual.source_chain_id {
        return Err("source_chain_id".into());
    }
    if expected.source_domain != actual.source_domain {
        return Err("source_domain".into());
    }
    if expected.destination_domain != actual.destination_domain {
        return Err("destination_domain".into());
    }
    if expected.amount_cctp_subunits != actual.amount_cctp_subunits {
        return Err("amount".into());
    }
    if expected.burn_token_bytes32 != actual.burn_token_bytes32 {
        return Err("burn_token".into());
    }
    if expected.mint_recipient_bytes32 != actual.mint_recipient_bytes32 {
        return Err("mint_recipient".into());
    }
    if expected.destination_caller_bytes32 != actual.destination_caller_bytes32 {
        return Err("destination_caller".into());
    }
    if expected.min_finality_threshold != actual.min_finality_threshold {
        return Err("finality".into());
    }
    if expected.token_messenger_bytes32 != actual.token_messenger_bytes32 {
        return Err("token_messenger".into());
    }
    if expected.sender != actual.sender {
        return Err("sender".into());
    }
    if expected.hook_data != actual.hook_data {
        return Err("hook_data".into());
    }
    Ok(())
}
