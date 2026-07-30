//! Circle CCTP v2 backend core (config, persistence, Iris, encoding, service).

pub mod approval;
pub mod attestation;
pub mod bounds;
pub mod builders;
pub mod config;
pub mod encoding;
pub mod evm_allowance;
pub mod evm_approval_verifier;
pub mod evm_burn_verifier;
pub mod evm_mint_verifier;
pub mod evm_rpc;
pub mod expectations;
pub mod fixtures;
pub mod iris;
pub mod message;
pub mod readiness;
pub mod service;
pub mod store;
pub mod transitions;
pub mod verifiers;

pub use config::CctpConfig;
pub use service::{CctpService, CctpServiceError};
