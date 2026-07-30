//! Unsigned CCTP wallet transaction builders (no signing/broadcast).

pub mod evm;
pub mod stellar;

use async_trait::async_trait;
use thiserror::Error;

use crate::cctp::config::CctpConfig;
use crate::cctp::store::CctpTransfer;
use crate::models::v2_cctp::PreparedWalletPayload;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BuilderError {
    #[error("not ready")]
    NotReady,
    #[error("quote expired")]
    QuoteExpired,
    #[error("fee quote expired")]
    FeeExpired,
    #[error("validation: {0}")]
    Validation(String),
    #[error("simulation failed: {0}")]
    SimulationFailed(String),
    #[error("encoding: {0}")]
    Encoding(String),
    #[error("account lookup: {0}")]
    AccountLookup(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedBurnBundle {
    pub primary: PreparedWalletPayload,
    pub required_approvals: Vec<PreparedWalletPayload>,
    pub required_prior_payloads: Vec<PreparedWalletPayload>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedMintBundle {
    pub primary: PreparedWalletPayload,
    pub expires_at: i64,
    pub payload_hash: String,
}

#[async_trait]
pub trait StellarCctpBurnBuilder: Send + Sync {
    fn is_ready(&self) -> bool;
    async fn prepare_burn(
        &self,
        transfer: &CctpTransfer,
        config: &CctpConfig,
    ) -> Result<PreparedBurnBundle, BuilderError>;
}

#[async_trait]
pub trait EvmCctpBurnBuilder: Send + Sync {
    fn is_ready(&self) -> bool;
    async fn prepare_burn(
        &self,
        transfer: &CctpTransfer,
        config: &CctpConfig,
    ) -> Result<PreparedBurnBundle, BuilderError>;
}

#[async_trait]
pub trait StellarCctpMintBuilder: Send + Sync {
    fn is_ready(&self) -> bool;
    async fn prepare_mint(
        &self,
        transfer: &CctpTransfer,
        config: &CctpConfig,
    ) -> Result<PreparedMintBundle, BuilderError>;
}

#[async_trait]
pub trait EvmCctpMintBuilder: Send + Sync {
    fn is_ready(&self) -> bool;
    async fn prepare_mint(
        &self,
        transfer: &CctpTransfer,
        config: &CctpConfig,
    ) -> Result<PreparedMintBundle, BuilderError>;
}

pub struct NotReadyStellarBurnBuilder;
#[async_trait]
impl StellarCctpBurnBuilder for NotReadyStellarBurnBuilder {
    fn is_ready(&self) -> bool {
        false
    }
    async fn prepare_burn(
        &self,
        _: &CctpTransfer,
        _: &CctpConfig,
    ) -> Result<PreparedBurnBundle, BuilderError> {
        Err(BuilderError::NotReady)
    }
}

pub struct NotReadyEvmBurnBuilder;
#[async_trait]
impl EvmCctpBurnBuilder for NotReadyEvmBurnBuilder {
    fn is_ready(&self) -> bool {
        false
    }
    async fn prepare_burn(
        &self,
        _: &CctpTransfer,
        _: &CctpConfig,
    ) -> Result<PreparedBurnBundle, BuilderError> {
        Err(BuilderError::NotReady)
    }
}

pub struct NotReadyStellarMintBuilder;
#[async_trait]
impl StellarCctpMintBuilder for NotReadyStellarMintBuilder {
    fn is_ready(&self) -> bool {
        false
    }
    async fn prepare_mint(
        &self,
        _: &CctpTransfer,
        _: &CctpConfig,
    ) -> Result<PreparedMintBundle, BuilderError> {
        Err(BuilderError::NotReady)
    }
}

pub struct NotReadyEvmMintBuilder;
#[async_trait]
impl EvmCctpMintBuilder for NotReadyEvmMintBuilder {
    fn is_ready(&self) -> bool {
        false
    }
    async fn prepare_mint(
        &self,
        _: &CctpTransfer,
        _: &CctpConfig,
    ) -> Result<PreparedMintBundle, BuilderError> {
        Err(BuilderError::NotReady)
    }
}
