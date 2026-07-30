//! Source-chain burn verification and destination mint verification traits.

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
    #[error("transient: {0}")]
    Transient(String),
    #[error("tx not found")]
    TxNotFound,
    #[error("verification failed: {0}")]
    Failed(String),
}

/// Outcome of destination mint verification — only `FailedRetryable` may transition retry state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MintVerifyOutcome {
    Pending,
    Succeeded,
    FailedRetryable { reason: String },
    NonceUsed,
}

/// Cryptographic/on-chain facts bound to a mint submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedMintFacts {
    pub tx_hash: String,
    pub destination_chain_id: String,
    pub contract_address: String,
    pub function_selector: String,
    pub message_hash: [u8; 32],
    pub attestation_hash: [u8; 32],
    pub nonce: String,
    pub payload_hash: String,
    pub outcome: MintVerifyOutcome,
    pub recipient_evidence: Option<String>,
}

impl VerifiedMintFacts {
    pub fn submission_ok(&self) -> bool {
        !self.payload_hash.is_empty()
            && matches!(
                self.outcome,
                MintVerifyOutcome::Pending
                    | MintVerifyOutcome::Succeeded
                    | MintVerifyOutcome::NonceUsed
            )
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedMintSubmission {
    pub tx_hash: String,
    pub targets_expected_contract: bool,
    pub message_bound: bool,
    pub attestation_bound: bool,
    pub nonce_used: Option<bool>,
    pub payload_hash: String,
}

#[async_trait]
pub trait StellarMintVerifier: Send + Sync {
    fn is_ready(&self) -> bool;
    async fn verify_mint_submission(
        &self,
        tx_hash: &str,
        message: &[u8],
        attestation: &[u8],
        nonce: &str,
        expected_payload_hash: &str,
    ) -> Result<VerifiedMintFacts, VerifierError>;
    async fn verify_mint_completion(
        &self,
        tx_hash: &str,
        nonce: &str,
        recipient: &str,
    ) -> Result<MintVerifyOutcome, VerifierError>;
}

#[async_trait]
pub trait EvmMintVerifier: Send + Sync {
    fn is_ready(&self) -> bool;
    async fn verify_mint_submission(
        &self,
        tx_hash: &str,
        message: &[u8],
        attestation: &[u8],
        nonce: &str,
        expected_payload_hash: &str,
    ) -> Result<VerifiedMintFacts, VerifierError>;
    async fn verify_mint_completion(
        &self,
        tx_hash: &str,
        nonce: &str,
        recipient: &str,
    ) -> Result<MintVerifyOutcome, VerifierError>;
}

/// Production placeholder — Stellar Soroban burn event parsing deferred.
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

pub struct NotReadyStellarMintVerifier;
#[async_trait]
impl StellarMintVerifier for NotReadyStellarMintVerifier {
    fn is_ready(&self) -> bool {
        false
    }
    async fn verify_mint_submission(
        &self,
        _: &str,
        _: &[u8],
        _: &[u8],
        _: &str,
        _: &str,
    ) -> Result<VerifiedMintFacts, VerifierError> {
        Err(VerifierError::NotReady)
    }
    async fn verify_mint_completion(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<MintVerifyOutcome, VerifierError> {
        Err(VerifierError::NotReady)
    }
}

pub struct NotReadyEvmMintVerifier;
#[async_trait]
impl EvmMintVerifier for NotReadyEvmMintVerifier {
    fn is_ready(&self) -> bool {
        false
    }
    async fn verify_mint_submission(
        &self,
        _: &str,
        _: &[u8],
        _: &[u8],
        _: &str,
        _: &str,
    ) -> Result<VerifiedMintFacts, VerifierError> {
        Err(VerifierError::NotReady)
    }
    async fn verify_mint_completion(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<MintVerifyOutcome, VerifierError> {
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

pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(data))
}

/// Deterministic fake for mint service-path tests.
pub struct FakeMintVerifier {
    pub facts: VerifiedMintFacts,
    pub completion: MintVerifyOutcome,
    pub ready: bool,
}

#[async_trait]
impl StellarMintVerifier for FakeMintVerifier {
    fn is_ready(&self) -> bool {
        self.ready
    }
    async fn verify_mint_submission(
        &self,
        tx_hash: &str,
        _message: &[u8],
        _attestation: &[u8],
        nonce: &str,
        expected_payload_hash: &str,
    ) -> Result<VerifiedMintFacts, VerifierError> {
        if !self.ready {
            return Err(VerifierError::NotReady);
        }
        if self.facts.tx_hash != tx_hash || self.facts.nonce != nonce {
            return Err(VerifierError::Failed("mint submission mismatch".into()));
        }
        if self.facts.payload_hash != expected_payload_hash {
            return Err(VerifierError::Failed("payload hash mismatch".into()));
        }
        Ok(self.facts.clone())
    }
    async fn verify_mint_completion(
        &self,
        tx_hash: &str,
        _nonce: &str,
        _recipient: &str,
    ) -> Result<MintVerifyOutcome, VerifierError> {
        if !self.ready {
            return Err(VerifierError::NotReady);
        }
        if self.facts.tx_hash != tx_hash {
            return Err(VerifierError::Failed("tx_hash mismatch".into()));
        }
        Ok(self.completion.clone())
    }
}

#[async_trait]
impl EvmMintVerifier for FakeMintVerifier {
    fn is_ready(&self) -> bool {
        self.ready
    }
    async fn verify_mint_submission(
        &self,
        tx_hash: &str,
        _message: &[u8],
        _attestation: &[u8],
        nonce: &str,
        expected_payload_hash: &str,
    ) -> Result<VerifiedMintFacts, VerifierError> {
        if !self.ready {
            return Err(VerifierError::NotReady);
        }
        if self.facts.tx_hash != tx_hash || self.facts.nonce != nonce {
            return Err(VerifierError::Failed("mint submission mismatch".into()));
        }
        if self.facts.payload_hash != expected_payload_hash {
            return Err(VerifierError::Failed("payload hash mismatch".into()));
        }
        Ok(self.facts.clone())
    }
    async fn verify_mint_completion(
        &self,
        tx_hash: &str,
        _nonce: &str,
        _recipient: &str,
    ) -> Result<MintVerifyOutcome, VerifierError> {
        if !self.ready {
            return Err(VerifierError::NotReady);
        }
        if self.facts.tx_hash != tx_hash {
            return Err(VerifierError::Failed("tx_hash mismatch".into()));
        }
        Ok(self.completion.clone())
    }
}
