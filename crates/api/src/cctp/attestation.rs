//! Attestation verification — production Circle CCTP v2 verifier and test seams.

use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use crate::cctp::attestation_crypto::{verify_attestation_signatures, AttestationCryptoError};
use crate::cctp::attester_set::{destination_for_message, AttesterDestination, AttesterSetCache};
use crate::cctp::config::{SEPOLIA_DOMAIN, STELLAR_TESTNET_DOMAIN};
use crate::cctp::iris_public_keys::{IrisPublicKeyCache, IrisPublicKeySource};
use crate::cctp::message::parse_cctp_v2_message;
use crate::metrics;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AttestationVerifyError {
    #[error("attestation verifier not ready")]
    NotReady,
    #[error("empty attestation")]
    Empty,
    #[error("empty message")]
    EmptyMessage,
    #[error("wrong corridor domains")]
    WrongCorridor,
    #[error("verification failed: {0}")]
    Invalid(String),
}

impl From<AttestationCryptoError> for AttestationVerifyError {
    fn from(e: AttestationCryptoError) -> Self {
        metrics::record_cctp_attestation_verify(e.reason_label());
        Self::Invalid(e.reason_label().into())
    }
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

/// Production Circle CCTP v2 attestation verifier.
pub struct CircleAttestationVerifier {
    iris_keys: Arc<IrisPublicKeyCache>,
    snapshots: Arc<AttesterSetCache>,
    iris_source: Arc<dyn IrisPublicKeySource>,
}

impl CircleAttestationVerifier {
    pub fn new(
        iris_keys: Arc<IrisPublicKeyCache>,
        snapshots: Arc<AttesterSetCache>,
        iris_source: Arc<dyn IrisPublicKeySource>,
    ) -> Self {
        Self {
            iris_keys,
            snapshots,
            iris_source,
        }
    }

    pub async fn bootstrap(&self) -> Result<(), AttestationVerifyError> {
        self.iris_keys
            .refresh(self.iris_source.as_ref())
            .await
            .map_err(|_e| AttestationVerifyError::NotReady)?;
        if self.iris_keys.is_stale_beyond_max() {
            return Err(AttestationVerifyError::NotReady);
        }
        Ok(())
    }

    fn snapshot_for_dest(
        &self,
        dest: AttesterDestination,
    ) -> Result<crate::cctp::attester_set::AttesterSetSnapshot, AttestationVerifyError> {
        let snap = self
            .snapshots
            .get(dest)
            .filter(|s| {
                s.is_fresh(self.snapshots.ttl) && !s.is_stale_beyond(self.snapshots.stale_max)
            })
            .ok_or(AttestationVerifyError::NotReady)?;
        Ok(snap)
    }

    fn verify_with_snapshot(
        raw_message: &[u8],
        attestation: &[u8],
        snap: &crate::cctp::attester_set::AttesterSetSnapshot,
    ) -> Result<(), AttestationVerifyError> {
        verify_attestation_signatures(
            raw_message,
            attestation,
            snap.signature_threshold,
            &snap.enabled_addresses,
        )?;
        metrics::record_cctp_attestation_verify("ok");
        Ok(())
    }
}

#[async_trait]
impl AttestationVerifier for CircleAttestationVerifier {
    fn is_ready(&self) -> bool {
        self.iris_keys.is_healthy() && self.snapshots.is_bidirectional_ready()
    }

    async fn verify_attestation(
        &self,
        raw_message: &[u8],
        attestation: &[u8],
    ) -> Result<(), AttestationVerifyError> {
        if raw_message.is_empty() {
            return Err(AttestationVerifyError::EmptyMessage);
        }
        if attestation.is_empty() {
            return Err(AttestationVerifyError::Empty);
        }
        if !self.is_ready() {
            return Err(AttestationVerifyError::NotReady);
        }

        let parsed = parse_cctp_v2_message(raw_message)
            .map_err(|_| AttestationVerifyError::Invalid("parse".into()))?;

        let dest = destination_for_message(parsed.source_domain, parsed.destination_domain)
            .ok_or(AttestationVerifyError::WrongCorridor)?;

        // Defense-in-depth corridor check
        let valid_pair = (parsed.source_domain == STELLAR_TESTNET_DOMAIN
            && parsed.destination_domain == SEPOLIA_DOMAIN)
            || (parsed.source_domain == SEPOLIA_DOMAIN
                && parsed.destination_domain == STELLAR_TESTNET_DOMAIN);
        if !valid_pair {
            return Err(AttestationVerifyError::WrongCorridor);
        }

        let snap = self.snapshot_for_dest(dest)?;

        match Self::verify_with_snapshot(raw_message, attestation, &snap) {
            Ok(()) => Ok(()),
            Err(AttestationVerifyError::Invalid(reason))
                if reason == AttestationCryptoError::UnknownSigner.reason_label() =>
            {
                // Single controlled refresh on unknown signer
                if self
                    .iris_keys
                    .refresh(self.iris_source.as_ref())
                    .await
                    .is_err()
                {
                    return Err(AttestationVerifyError::NotReady);
                }
                let snap = self.snapshot_for_dest(dest)?;
                Self::verify_with_snapshot(raw_message, attestation, &snap)
            }
            other => other,
        }
    }
}

/// Production default — wired only when full attestation stack is configured.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cctp::fixtures::circle_attestation_v2::{
        ATTESTER_ADDRESS_1, ATTESTER_ADDRESS_2, FIXTURE_VALID_ATTESTATION, FIXTURE_VALID_MESSAGE,
    };

    #[tokio::test]
    async fn verifies_official_fixture_via_crypto() {
        let enabled = vec![ATTESTER_ADDRESS_1, ATTESTER_ADDRESS_2];
        let mut sorted = enabled.clone();
        sorted.sort();
        crate::cctp::attestation_crypto::verify_attestation_signatures(
            FIXTURE_VALID_MESSAGE,
            FIXTURE_VALID_ATTESTATION,
            2,
            &sorted,
        )
        .expect("crypto path");
    }

    #[tokio::test]
    async fn rejects_when_not_ready() {
        let verifier = NotReadyAttestationVerifier;
        let err = verifier.verify_attestation(&[1], &[2]).await.unwrap_err();
        assert_eq!(err, AttestationVerifyError::NotReady);
    }
}
