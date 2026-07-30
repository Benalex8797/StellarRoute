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

    /// Wire production EVM RPC builders/verifiers and attestation stack when configured.
    /// Stellar burn/approval/mint remain NotReady; `is_public_executable` stays false.
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
        if let Some(verifier) = try_build_attestation_verifier(config) {
            runtime.attestation_verifier = verifier;
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

fn try_build_attestation_verifier(
    config: &CctpConfig,
) -> Option<Arc<dyn crate::cctp::attestation::AttestationVerifier>> {
    use crate::cctp::attestation::CircleAttestationVerifier;
    use crate::cctp::attester_set::AttesterSetCache;
    use crate::cctp::evm_attester_reader::evm_reader_arc;
    use crate::cctp::iris_public_keys::{IrisPublicKeyCache, ReqwestIrisPublicKeySource};
    use crate::cctp::stellar_attester_reader::stellar_reader_arc;

    if config.sepolia_rpc_url.trim().is_empty() || config.stellar_rpc_url.trim().is_empty() {
        return None;
    }
    let iris_source = ReqwestIrisPublicKeySource::from_config(config).ok()?;
    let evm_reader = evm_reader_arc(config).ok()?;
    let stellar_reader = stellar_reader_arc(config).ok()?;

    let iris_keys = Arc::new(IrisPublicKeyCache::from_config(config));
    let snapshots = Arc::new(AttesterSetCache::from_config(config));
    let verifier = Arc::new(CircleAttestationVerifier::new(
        iris_keys.clone(),
        snapshots.clone(),
        Arc::new(iris_source),
    ));

    // Bootstrap synchronously via tokio runtime if available, else defer to first verify.
    let readers: Vec<Arc<dyn crate::cctp::attester_set::AttesterSetReader>> =
        vec![evm_reader, stellar_reader];
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let iris_keys_c = iris_keys.clone();
        let snapshots_c = snapshots.clone();
        let verifier_c = verifier.clone();
        let readers_c = readers.clone();
        handle.block_on(async {
            if verifier_c.bootstrap().await.is_err() {
                return;
            }
            let _ = snapshots_c.refresh_all(&readers_c, &iris_keys_c).await;
        });
        if !verifier.is_ready() {
            return None;
        }
    } else {
        return None;
    }

    Some(verifier)
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
