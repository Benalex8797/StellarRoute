//! Circle CCTP v2 backend core (config, persistence, Iris, encoding, service).

pub mod attestation;
pub mod bounds;
pub mod builders;
pub mod config;
pub mod encoding;
pub mod evm_burn_verifier;
pub mod expectations;
pub mod iris;
pub mod message;
pub mod readiness;
pub mod service;
pub mod store;
pub mod transitions;
pub mod verifiers;

pub use config::CctpConfig;
pub use service::{CctpService, CctpServiceError};
