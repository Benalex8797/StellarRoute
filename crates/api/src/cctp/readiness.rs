//! CCTP component readiness aggregation — no trait defaults may report ready.

use std::sync::Arc;

use crate::cctp::attestation::AttestationVerifier;
use crate::cctp::builders::{
    EvmCctpBurnBuilder, EvmCctpMintBuilder, StellarCctpBurnBuilder, StellarCctpMintBuilder,
};
use crate::cctp::config::CctpConfig;
use crate::cctp::verifiers::{
    EvmBurnVerifier, EvmMintVerifier, StellarBurnVerifier, StellarMintVerifier,
};
use crate::models::v2_cctp::CctpDirection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadinessComponent {
    StellarBurnBuilder,
    EvmBurnBuilder,
    StellarMintBuilder,
    EvmMintBuilder,
    StellarBurnVerifier,
    EvmBurnVerifier,
    StellarMintVerifier,
    EvmMintVerifier,
    AttestationVerifier,
}

#[derive(Debug, Clone, Default)]
pub struct CctpReadiness {
    pub missing: Vec<ReadinessComponent>,
}

impl CctpReadiness {
    pub fn is_ready(&self) -> bool {
        self.missing.is_empty()
    }
}

pub struct CctpRuntime {
    pub stellar_burn_builder: Arc<dyn StellarCctpBurnBuilder>,
    pub evm_burn_builder: Arc<dyn EvmCctpBurnBuilder>,
    pub stellar_mint_builder: Arc<dyn StellarCctpMintBuilder>,
    pub evm_mint_builder: Arc<dyn EvmCctpMintBuilder>,
    pub stellar_burn_verifier: Arc<dyn StellarBurnVerifier>,
    pub evm_burn_verifier: Arc<dyn EvmBurnVerifier>,
    pub stellar_mint_verifier: Arc<dyn StellarMintVerifier>,
    pub evm_mint_verifier: Arc<dyn EvmMintVerifier>,
    pub attestation_verifier: Arc<dyn AttestationVerifier>,
}

impl CctpRuntime {
    pub fn production_defaults() -> Self {
        use crate::cctp::attestation::NotReadyAttestationVerifier;
        use crate::cctp::builders::{
            NotReadyEvmBurnBuilder, NotReadyEvmMintBuilder, NotReadyStellarBurnBuilder,
            NotReadyStellarMintBuilder,
        };
        use crate::cctp::verifiers::{
            NotReadyEvmBurnVerifier, NotReadyEvmMintVerifier, NotReadyStellarBurnVerifier,
            NotReadyStellarMintVerifier,
        };

        Self {
            stellar_burn_builder: Arc::new(NotReadyStellarBurnBuilder),
            evm_burn_builder: Arc::new(NotReadyEvmBurnBuilder),
            stellar_mint_builder: Arc::new(NotReadyStellarMintBuilder),
            evm_mint_builder: Arc::new(NotReadyEvmMintBuilder),
            stellar_burn_verifier: Arc::new(NotReadyStellarBurnVerifier),
            evm_burn_verifier: Arc::new(NotReadyEvmBurnVerifier),
            stellar_mint_verifier: Arc::new(NotReadyStellarMintVerifier),
            evm_mint_verifier: Arc::new(NotReadyEvmMintVerifier),
            attestation_verifier: Arc::new(NotReadyAttestationVerifier),
        }
    }

    pub fn for_tests(
        stellar_burn: Arc<dyn StellarBurnVerifier>,
        evm_burn: Arc<dyn EvmBurnVerifier>,
        attestation: Arc<dyn AttestationVerifier>,
    ) -> Self {
        Self {
            stellar_burn_verifier: stellar_burn,
            evm_burn_verifier: evm_burn,
            attestation_verifier: attestation,
            ..Self::production_defaults()
        }
    }

    pub fn assess(&self, direction: CctpDirection) -> CctpReadiness {
        let mut missing = Vec::new();
        match direction {
            CctpDirection::StellarToEvm => {
                if !self.stellar_burn_builder.is_ready() {
                    missing.push(ReadinessComponent::StellarBurnBuilder);
                }
                if !self.stellar_burn_verifier.is_ready() {
                    missing.push(ReadinessComponent::StellarBurnVerifier);
                }
                if !self.evm_mint_builder.is_ready() {
                    missing.push(ReadinessComponent::EvmMintBuilder);
                }
                if !self.evm_mint_verifier.is_ready() {
                    missing.push(ReadinessComponent::EvmMintVerifier);
                }
            }
            CctpDirection::EvmToStellar => {
                if !self.evm_burn_builder.is_ready() {
                    missing.push(ReadinessComponent::EvmBurnBuilder);
                }
                if !self.evm_burn_verifier.is_ready() {
                    missing.push(ReadinessComponent::EvmBurnVerifier);
                }
                if !self.stellar_mint_builder.is_ready() {
                    missing.push(ReadinessComponent::StellarMintBuilder);
                }
                if !self.stellar_mint_verifier.is_ready() {
                    missing.push(ReadinessComponent::StellarMintVerifier);
                }
            }
        }
        if !self.attestation_verifier.is_ready() {
            missing.push(ReadinessComponent::AttestationVerifier);
        }
        CctpReadiness { missing }
    }

    pub fn is_public_executable(&self, config: &CctpConfig) -> bool {
        config.enabled
            && config.is_configured()
            && self.stellar_burn_builder.is_ready()
            && self.evm_burn_builder.is_ready()
            && self.stellar_mint_builder.is_ready()
            && self.evm_mint_builder.is_ready()
            && self.stellar_burn_verifier.is_ready()
            && self.evm_burn_verifier.is_ready()
            && self.stellar_mint_verifier.is_ready()
            && self.evm_mint_verifier.is_ready()
            && self.attestation_verifier.is_ready()
    }
}
