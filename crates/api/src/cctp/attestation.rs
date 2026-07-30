//! Attestation verification — production Circle CCTP v2 verifier and test seams.

use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::task::JoinHandle;

use crate::cctp::attestation_crypto::{verify_attestation_signatures, AttestationCryptoError};
use crate::cctp::attestation_trust::{
    AttestationRefreshDeps, AttestationTrustCache, AttestationTrustError,
};
use crate::cctp::attester_set::{destination_for_message, AttesterSetSnapshot};
use crate::cctp::config::{SEPOLIA_DOMAIN, STELLAR_TESTNET_DOMAIN};
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
    pub(crate) trust: Arc<AttestationTrustCache>,
    deps: Arc<AttestationRefreshDeps>,
    _refresh_task: Option<JoinHandle<()>>,
}

impl CircleAttestationVerifier {
    pub fn new(trust: Arc<AttestationTrustCache>, deps: AttestationRefreshDeps) -> Self {
        let deps = Arc::new(deps);
        let weak = Arc::downgrade(&trust);
        let refresh_interval = trust.ttl() / 2;
        let refresh_task = Some(AttestationTrustCache::spawn_background_refresh(
            weak,
            deps.clone(),
            refresh_interval.max(std::time::Duration::from_secs(30)),
        ));
        Self {
            trust,
            deps,
            _refresh_task: refresh_task,
        }
    }

    pub async fn bootstrap(&self) -> Result<(), AttestationVerifyError> {
        self.trust
            .full_refresh(self.deps.as_ref())
            .await
            .map_err(|_| AttestationVerifyError::NotReady)?;
        if !self.trust.is_ready() {
            return Err(AttestationVerifyError::NotReady);
        }
        Ok(())
    }

    pub(crate) fn verify_with_snapshot(
        raw_message: &[u8],
        attestation: &[u8],
        snap: &AttesterSetSnapshot,
        iris_set_hash: [u8; 32],
    ) -> Result<(), AttestationVerifyError> {
        if snap.iris_set_hash != iris_set_hash {
            return Err(AttestationVerifyError::NotReady);
        }
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
        self.trust.is_ready()
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

        self.trust
            .ensure_fresh(self.deps.as_ref())
            .await
            .map_err(|e| match e {
                AttestationTrustError::Stale | AttestationTrustError::NotReady => {
                    AttestationVerifyError::NotReady
                }
                _ => AttestationVerifyError::NotReady,
            })?;

        let parsed = parse_cctp_v2_message(raw_message)
            .map_err(|_| AttestationVerifyError::Invalid("parse".into()))?;

        let dest = destination_for_message(parsed.source_domain, parsed.destination_domain)
            .ok_or(AttestationVerifyError::WrongCorridor)?;

        let valid_pair = (parsed.source_domain == STELLAR_TESTNET_DOMAIN
            && parsed.destination_domain == SEPOLIA_DOMAIN)
            || (parsed.source_domain == SEPOLIA_DOMAIN
                && parsed.destination_domain == STELLAR_TESTNET_DOMAIN);
        if !valid_pair {
            return Err(AttestationVerifyError::WrongCorridor);
        }

        let generation = self
            .trust
            .generation()
            .ok_or(AttestationVerifyError::NotReady)?;
        let snap = self
            .trust
            .snapshot_for(dest)
            .ok_or(AttestationVerifyError::NotReady)?;
        let iris_hash = generation.iris.set_hash;

        match Self::verify_with_snapshot(raw_message, attestation, &snap, iris_hash) {
            Ok(()) => Ok(()),
            Err(AttestationVerifyError::Invalid(reason))
                if reason == AttestationCryptoError::UnknownSigner.reason_label() =>
            {
                self.trust
                    .full_refresh(self.deps.as_ref())
                    .await
                    .map_err(|_| AttestationVerifyError::NotReady)?;
                let generation = self
                    .trust
                    .generation()
                    .ok_or(AttestationVerifyError::NotReady)?;
                let snap = self
                    .trust
                    .snapshot_for(dest)
                    .ok_or(AttestationVerifyError::NotReady)?;
                Self::verify_with_snapshot(
                    raw_message,
                    attestation,
                    &snap,
                    generation.iris.set_hash,
                )
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
    use crate::cctp::attestation_trust::{AttestationTrustCache, MockClock, SystemClock};
    use crate::cctp::attester_set::{
        AttesterDestination, AttesterSetError, AttesterSetReader, RawOnChainAttesterSet,
    };
    use crate::cctp::fixtures::circle_attestation_v2::{
        ATTESTER_ADDRESS_1, ATTESTER_ADDRESS_2, ATTESTER_ADDRESS_3, FIXTURE_VALID_ATTESTATION,
        FIXTURE_VALID_MESSAGE,
    };
    use crate::cctp::iris_public_keys::{IrisPublicKeyError, IrisPublicKeySource};
    use async_trait::async_trait;
    use std::time::Duration;

    #[tokio::test]
    async fn verifies_official_fixture_via_crypto() {
        let enabled = vec![ATTESTER_ADDRESS_1, ATTESTER_ADDRESS_2, ATTESTER_ADDRESS_3];
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

    struct StaticIris(Vec<[u8; 20]>);

    #[async_trait]
    impl IrisPublicKeySource for StaticIris {
        async fn fetch_public_keys(&self) -> Result<Vec<[u8; 20]>, IrisPublicKeyError> {
            Ok(self.0.clone())
        }
    }

    struct StaticReader {
        dest: AttesterDestination,
        enabled: Vec<[u8; 20]>,
    }

    #[async_trait]
    impl AttesterSetReader for StaticReader {
        fn destination(&self) -> AttesterDestination {
            self.dest
        }

        async fn read_on_chain_set(&self) -> Result<RawOnChainAttesterSet, AttesterSetError> {
            Ok(RawOnChainAttesterSet {
                signature_threshold: 2,
                enabled_addresses: self.enabled.clone(),
                block_or_ledger: Some("mock".into()),
            })
        }
    }

    fn e2e_verifier(enabled: Vec<[u8; 20]>) -> CircleAttestationVerifier {
        let iris = enabled.clone();
        let trust = Arc::new(AttestationTrustCache::new(
            Duration::from_secs(900),
            Duration::from_secs(86_400),
            Arc::new(SystemClock),
        ));
        CircleAttestationVerifier::new(
            trust,
            AttestationRefreshDeps {
                iris_source: Arc::new(StaticIris(iris)),
                readers: vec![
                    Arc::new(StaticReader {
                        dest: AttesterDestination::Sepolia,
                        enabled: enabled.clone(),
                    }),
                    Arc::new(StaticReader {
                        dest: AttesterDestination::StellarTestnet,
                        enabled,
                    }),
                ],
            },
        )
    }

    #[tokio::test]
    async fn e2e_bootstrap_and_verify_with_snapshot_both_destinations() {
        let enabled = vec![ATTESTER_ADDRESS_1, ATTESTER_ADDRESS_2, ATTESTER_ADDRESS_3];
        let verifier = e2e_verifier(enabled);
        verifier.bootstrap().await.expect("bootstrap");
        assert!(verifier.is_ready());

        let generation = verifier.trust.generation().expect("generation");
        for dest in [
            AttesterDestination::Sepolia,
            AttesterDestination::StellarTestnet,
        ] {
            let snap = verifier.trust.snapshot_for(dest).expect("snapshot");
            CircleAttestationVerifier::verify_with_snapshot(
                FIXTURE_VALID_MESSAGE,
                FIXTURE_VALID_ATTESTATION,
                &snap,
                generation.iris.set_hash,
            )
            .expect("verify both destinations");
        }
    }

    #[tokio::test]
    async fn e2e_official_fixture_rejected_by_corridor_gate() {
        let enabled = vec![ATTESTER_ADDRESS_1, ATTESTER_ADDRESS_2, ATTESTER_ADDRESS_3];
        let verifier = e2e_verifier(enabled);
        verifier.bootstrap().await.unwrap();
        let err = verifier
            .verify_attestation(FIXTURE_VALID_MESSAGE, FIXTURE_VALID_ATTESTATION)
            .await
            .unwrap_err();
        assert_eq!(err, AttestationVerifyError::WrongCorridor);
    }

    #[tokio::test]
    async fn e2e_rejects_stale_snapshot() {
        let enabled = vec![ATTESTER_ADDRESS_1, ATTESTER_ADDRESS_2, ATTESTER_ADDRESS_3];
        let clock = Arc::new(MockClock::new());
        let trust = Arc::new(AttestationTrustCache::new(
            Duration::from_secs(60),
            Duration::from_secs(120),
            clock.clone(),
        ));
        let verifier = CircleAttestationVerifier::new(
            trust,
            AttestationRefreshDeps {
                iris_source: Arc::new(StaticIris(enabled.clone())),
                readers: vec![
                    Arc::new(StaticReader {
                        dest: AttesterDestination::Sepolia,
                        enabled: enabled.clone(),
                    }),
                    Arc::new(StaticReader {
                        dest: AttesterDestination::StellarTestnet,
                        enabled,
                    }),
                ],
            },
        );
        verifier.bootstrap().await.unwrap();
        clock.advance(Duration::from_secs(121));
        let err = verifier
            .verify_attestation(FIXTURE_VALID_MESSAGE, FIXTURE_VALID_ATTESTATION)
            .await
            .unwrap_err();
        assert_eq!(err, AttestationVerifyError::NotReady);
    }

    #[tokio::test]
    async fn e2e_unknown_signer_refresh_success() {
        let enabled = vec![ATTESTER_ADDRESS_1, ATTESTER_ADDRESS_2, ATTESTER_ADDRESS_3];
        let verifier = e2e_verifier(enabled);
        verifier.bootstrap().await.unwrap();
        let generation = verifier.trust.generation().unwrap();
        let snap = verifier
            .trust
            .snapshot_for(AttesterDestination::Sepolia)
            .unwrap();
        CircleAttestationVerifier::verify_with_snapshot(
            FIXTURE_VALID_MESSAGE,
            FIXTURE_VALID_ATTESTATION,
            &snap,
            generation.iris.set_hash,
        )
        .unwrap();
    }
}
