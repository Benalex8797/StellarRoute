//! Attestation verification seam — production defaults fail closed.

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AttestationVerifyError {
    #[error("attestation verifier not ready")]
    NotReady,
    #[error("empty attestation")]
    Empty,
    #[error("empty message")]
    EmptyMessage,
    #[error("verification failed: {0}")]
    Invalid(String),
}

#[async_trait]
pub trait AttestationVerifier: Send + Sync {
    fn is_ready(&self) -> bool;
    async fn verify_attestation(
        &self,
        raw_message: &[u8],
        attestation: &[u8],
    ) -> Result<(), AttestationVerifyError>;
}

/// Production default — cryptographic Circle signature verification deferred.
pub struct NotReadyAttestationVerifier;

#[async_trait]
impl AttestationVerifier for NotReadyAttestationVerifier {
    fn is_ready(&self) -> bool {
        false
    }

    async fn verify_attestation(
        &self,
        _raw_message: &[u8],
        _attestation: &[u8],
    ) -> Result<(), AttestationVerifyError> {
        Err(AttestationVerifyError::NotReady)
    }
}

/// Test-only verifier accepting non-empty message + attestation pairs.
pub struct FakeAttestationVerifier {
    pub ready: bool,
}

#[async_trait]
impl AttestationVerifier for FakeAttestationVerifier {
    fn is_ready(&self) -> bool {
        self.ready
    }

    async fn verify_attestation(
        &self,
        raw_message: &[u8],
        attestation: &[u8],
    ) -> Result<(), AttestationVerifyError> {
        if !self.ready {
            return Err(AttestationVerifyError::NotReady);
        }
        if raw_message.is_empty() {
            return Err(AttestationVerifyError::EmptyMessage);
        }
        if attestation.is_empty() {
            return Err(AttestationVerifyError::Empty);
        }
        Ok(())
    }
}
