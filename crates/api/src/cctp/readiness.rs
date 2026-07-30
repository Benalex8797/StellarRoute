//! CCTP component readiness aggregation — no trait defaults may report ready.

use std::sync::Arc;

use crate::cctp::approval::{EvmApprovalVerifier, StellarApprovalVerifier};
use crate::cctp::attestation::AttestationVerifier;
use crate::cctp::builders::evm::{ProductionEvmCctpBuilder, SharedProductionEvmBuilder};
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
    EvmApprovalVerifier,
    StellarApprovalVerifier,
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
    pub evm_approval_verifier: Arc<dyn EvmApprovalVerifier>,
    pub stellar_approval_verifier: Arc<dyn StellarApprovalVerifier>,
    pub attestation_verifier: Arc<dyn AttestationVerifier>,
}

impl CctpRuntime {
    pub fn production_defaults() -> Self {
        use crate::cctp::approval::{NotReadyEvmApprovalVerifier, NotReadyStellarApprovalVerifier};
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
            evm_approval_verifier: Arc::new(NotReadyEvmApprovalVerifier),
            stellar_approval_verifier: Arc::new(NotReadyStellarApprovalVerifier),
            attestation_verifier: Arc::new(NotReadyAttestationVerifier),
        }
    }

    /// Wire production EVM RPC builders/verifiers when `sepolia_rpc_url` is configured.
    /// Stellar + attestation remain NotReady; `is_public_executable` stays false.
    pub fn from_config(config: &CctpConfig) -> Self {
        let mut runtime = Self::production_defaults();
        if !config.sepolia_rpc_url.trim().is_empty() {
            let shared =
                SharedProductionEvmBuilder(Arc::new(ProductionEvmCctpBuilder::from_config(config)));
            runtime.evm_burn_builder = Arc::new(shared.clone());
            runtime.evm_mint_builder = Arc::new(shared);
            if let Ok(v) = crate::cctp::evm_approval_verifier::EvmRpcApprovalVerifier::new(config) {
                runtime.evm_approval_verifier = Arc::new(v);
            }
            if let Ok(v) = crate::cctp::evm_burn_verifier::EvmRpcBurnVerifier::new(config) {
                runtime.evm_burn_verifier = Arc::new(v);
            }
            if let Ok(v) = crate::cctp::evm_mint_verifier::EvmRpcMintVerifier::new(config) {
                runtime.evm_mint_verifier = Arc::new(v);
            }
        }
        runtime
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
                if !self.stellar_approval_verifier.is_ready() {
                    missing.push(ReadinessComponent::StellarApprovalVerifier);
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
                if !self.evm_approval_verifier.is_ready() {
                    missing.push(ReadinessComponent::EvmApprovalVerifier);
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
            && self.evm_approval_verifier.is_ready()
            && self.stellar_approval_verifier.is_ready()
            && self.attestation_verifier.is_ready()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_defaults_all_not_ready() {
        let rt = CctpRuntime::production_defaults();
        assert!(!rt.evm_burn_builder.is_ready());
        assert!(!rt.evm_mint_builder.is_ready());
        assert!(!rt.evm_burn_verifier.is_ready());
        assert!(!rt.evm_approval_verifier.is_ready());
        assert!(!rt.is_public_executable(&CctpConfig::default_testnet()));
    }

    #[test]
    fn from_config_wires_evm_when_rpc_present() {
        let mut cfg = CctpConfig::default_testnet();
        cfg.sepolia_rpc_url = "https://rpc.sepolia.org".into();
        let rt = CctpRuntime::from_config(&cfg);
        assert!(rt.evm_burn_builder.is_ready());
        assert!(rt.evm_mint_builder.is_ready());
        assert!(rt.evm_burn_verifier.is_ready());
        assert!(rt.evm_mint_verifier.is_ready());
        assert!(rt.evm_approval_verifier.is_ready());
        assert!(!rt.stellar_burn_builder.is_ready());
        assert!(!rt.is_public_executable(&cfg));
    }

    #[test]
    fn evm_to_stellar_assess_requires_approval_verifier() {
        let mut cfg = CctpConfig::default_testnet();
        cfg.sepolia_rpc_url = "https://rpc.sepolia.org".into();
        let rt = CctpRuntime::from_config(&cfg);
        let readiness = rt.assess(CctpDirection::EvmToStellar);
        assert!(!readiness
            .missing
            .contains(&ReadinessComponent::EvmApprovalVerifier));
        assert!(readiness
            .missing
            .contains(&ReadinessComponent::StellarMintVerifier));
    }
}
