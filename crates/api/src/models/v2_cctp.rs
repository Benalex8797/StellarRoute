//! Circle CCTP v2 bridge wire models (contract freeze — no backend execution).
//!
//! Discriminated, snake_case JSON shapes for the first testnet corridor
//! (Stellar testnet domain 27 <-> Ethereum Sepolia domain 0). Handlers remain
//! fail-closed until a later implementation phase wires protocol execution.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::v2::ChainAssetV2;

/// Provider identifier for Circle CCTP v2.
pub const CCTP_PROVIDER_ID: &str = "circle-cctp";

/// Documented testnet corridor id (metadata only; not executable on this branch).
pub const CCTP_TESTNET_CORRIDOR_ID: &str = "circle-cctp:usdc:stellar-testnet:ethereum-sepolia";

/// Bridge transfer direction for the Stellar <-> EVM corridor.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CctpDirection {
    StellarToEvm,
    EvmToStellar,
}

/// CCTP finality mode. Stellar outbound burns must use `standard` only.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CctpFinality {
    Standard,
    Fast,
}

/// Saga lifecycle for a CCTP transfer (distinct from HTTP error codes).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CctpTransferStatus {
    Created,
    BurnPrepared,
    BurnSubmitted,
    AwaitingAttestation,
    AttestationReady,
    MintPrepared,
    MintSubmitted,
    Completed,
    AttestationFailed,
    MintFailedRetryable,
    Cancelled,
    ProviderKilled,
}

/// Advertised corridor capability (empty by default until backend health gates execution).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SupportedCorridor {
    pub corridor_id: String,
    pub provider: String,
    pub direction: CctpDirection,
    pub source_chain_id: String,
    pub destination_chain_id: String,
    pub source_asset: ChainAssetV2,
    pub destination_asset: ChainAssetV2,
    /// Always false on the contract-freeze branch.
    pub executable: bool,
}

/// Runtime fee quote fields — no invented fixed fees.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CctpFeeQuote {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_fee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination_fee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge_fee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_asset: Option<ChainAssetV2>,
}

/// Prepared wallet payload union returned by prepare-burn / prepare-mint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PreparedWalletPayload {
    StellarXdr {
        network_passphrase: String,
        xdr_envelope: String,
    },
    EvmTransaction {
        chain_id: String,
        to: String,
        data: String,
        value: String,
    },
}

/// Typed status/error details on transfer polling responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CctpStatusDetails {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
}

/// `POST /api/v2/bridge/cctp/quote` request body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CctpQuoteRequest {
    pub corridor_id: String,
    pub provider: String,
    pub direction: CctpDirection,
    pub source_chain_id: String,
    pub destination_chain_id: String,
    pub source_asset: ChainAssetV2,
    pub destination_asset: ChainAssetV2,
    /// Decimal string amount (never float).
    pub amount: String,
    pub recipient: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
    pub finality: CctpFinality,
}

impl CctpQuoteRequest {
    /// Contract validation shared by handlers and unit tests.
    pub fn validate(&self) -> Result<(), CctpValidationError> {
        if self.provider != CCTP_PROVIDER_ID {
            return Err(CctpValidationError::UnsupportedCorridor);
        }
        if self.corridor_id.is_empty() {
            return Err(CctpValidationError::UnsupportedCorridor);
        }
        if self.amount.trim().is_empty() {
            return Err(CctpValidationError::InvalidAmount);
        }
        if self.recipient.trim().is_empty() {
            return Err(CctpValidationError::InvalidRecipient);
        }
        if self.direction == CctpDirection::StellarToEvm && self.finality == CctpFinality::Fast {
            return Err(CctpValidationError::InvalidFinality);
        }
        Ok(())
    }
}

/// Validation failures surfaced before fail-closed `cctp_not_enabled`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CctpValidationError {
    UnsupportedCorridor,
    InvalidFinality,
    InvalidRecipient,
    InvalidAmount,
}

/// `POST /api/v2/bridge/cctp/quote` success response (not returned until enabled).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CctpQuoteResponse {
    pub transfer_id: String,
    pub corridor_id: String,
    pub provider: String,
    pub direction: CctpDirection,
    pub source_amount: String,
    pub destination_amount: String,
    pub fee_quote: CctpFeeQuote,
    pub expires_at: i64,
    pub finality: CctpFinality,
}

/// `GET /api/v2/bridge/cctp/{transfer_id}` response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CctpTransferStatusResponse {
    pub transfer_id: String,
    pub corridor_id: String,
    pub provider: String,
    pub direction: CctpDirection,
    pub status: CctpTransferStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_tx_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination_tx_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support_reference_id: Option<String>,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CctpStatusDetails>,
}

/// `POST /api/v2/bridge/cctp/{transfer_id}/prepare-burn` response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CctpPrepareBurnResponse {
    pub transfer_id: String,
    pub status: CctpTransferStatus,
    pub payload: PreparedWalletPayload,
    pub expires_at: i64,
}

/// Burn submit accepts only an on-chain tx hash acknowledgement.
///
/// Signed transaction broadcasting is the wallet/provider responsibility;
/// the API records the hash for attestation polling and later verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CctpSubmitBurnRequest {
    pub tx_hash: String,
}

/// `POST /api/v2/bridge/cctp/{transfer_id}/submit-burn` response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CctpSubmitBurnResponse {
    pub transfer_id: String,
    pub status: CctpTransferStatus,
    pub source_tx_hash: String,
}

/// `POST /api/v2/bridge/cctp/{transfer_id}/prepare-mint` response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CctpPrepareMintResponse {
    pub transfer_id: String,
    pub status: CctpTransferStatus,
    pub payload: PreparedWalletPayload,
    pub expires_at: i64,
}

/// Mint submit accepts only an on-chain tx hash acknowledgement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CctpSubmitMintRequest {
    pub tx_hash: String,
}

/// `POST /api/v2/bridge/cctp/{transfer_id}/submit-mint` response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CctpSubmitMintResponse {
    pub transfer_id: String,
    pub status: CctpTransferStatus,
    pub destination_tx_hash: String,
}

/// `POST /api/v2/bridge/cctp/{transfer_id}/reattest` response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CctpReattestResponse {
    pub transfer_id: String,
    pub status: CctpTransferStatus,
    pub retryable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_quote(direction: CctpDirection, finality: CctpFinality) -> CctpQuoteRequest {
        CctpQuoteRequest {
            corridor_id: CCTP_TESTNET_CORRIDOR_ID.into(),
            provider: CCTP_PROVIDER_ID.into(),
            direction,
            source_chain_id: "stellar:testnet".into(),
            destination_chain_id: "eip155:11155111".into(),
            source_asset: ChainAssetV2 {
                chain_id: "stellar:testnet".into(),
                asset: "erc20:CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA".into(),
                canonical:
                    "stellar:testnet/erc20:CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA"
                        .into(),
                symbol: Some("USDC".into()),
            },
            destination_asset: ChainAssetV2 {
                chain_id: "eip155:11155111".into(),
                asset: "erc20:0x1c7d4b196cb0c7b01d743fbc6116a902379c7238".into(),
                canonical: "eip155:11155111/erc20:0x1c7d4b196cb0c7b01d743fbc6116a902379c7238"
                    .into(),
                symbol: Some("USDC".into()),
            },
            amount: "10.0".into(),
            recipient: "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0".into(),
            sender: None,
            finality,
        }
    }

    #[test]
    fn rejects_stellar_source_fast_finality() {
        let req = base_quote(CctpDirection::StellarToEvm, CctpFinality::Fast);
        assert_eq!(req.validate(), Err(CctpValidationError::InvalidFinality));
    }

    #[test]
    fn allows_evm_source_fast_finality_at_validation_layer() {
        let req = base_quote(CctpDirection::EvmToStellar, CctpFinality::Fast);
        assert!(req.validate().is_ok());
    }

    #[test]
    fn rejects_unknown_fields_on_quote_request() {
        let json = r#"{
            "corridor_id":"x","provider":"circle-cctp","direction":"stellar_to_evm",
            "source_chain_id":"stellar:testnet","destination_chain_id":"eip155:11155111",
            "source_asset":{"chain_id":"stellar:testnet","asset":"erc20:CB","canonical":"stellar:testnet/erc20:CB"},
            "destination_asset":{"chain_id":"eip155:11155111","asset":"erc20:0x1","canonical":"eip155:11155111/erc20:0x1"},
            "amount":"1","recipient":"0xabc","finality":"standard","extra":true
        }"#;
        assert!(serde_json::from_str::<CctpQuoteRequest>(json).is_err());
    }
}
