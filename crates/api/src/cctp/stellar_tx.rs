//! Stellar Soroban transaction fetch, finality, envelope invoke decode.
//!
//! Uses pinned Soroban JSON-RPC `getTransaction` (HTTPS host-locked client in `stellar_rpc`).

use serde::Deserialize;
use serde_json::json;
use stellar_xdr::curr::{
    ContractEvent, HostFunction, InvokeContractArgs, Limits, OperationBody, ReadXdr, ScAddress,
    ScBytes, ScVal, TransactionEnvelope, TransactionV1Envelope, Uint256,
};

use crate::cctp::bounds::{check_str_len, MAX_TX_HASH_LEN};
use crate::cctp::config::{CctpConfig, STELLAR_TESTNET_PASSPHRASE};
use crate::cctp::encoding::stellar_contract_to_bytes32;
use crate::cctp::stellar_contract_events::collect_contract_events;
use crate::cctp::stellar_rpc::{check_rpc_response_len, StellarRpcClient};
use crate::cctp::verifiers::VerifierError;
use crate::models::v2_cctp::STELLAR_TESTNET_CHAIN_ID;

pub const MIN_LEDGER_CONFIRMATIONS: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizedTx {
    pub tx_hash: String,
    pub status: TxStatus,
    pub ledger: u32,
    pub created_at: Option<String>,
    pub envelope_xdr: String,
    pub contract_events: Vec<ContractEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxStatus {
    Success,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedInvoke {
    pub source_account: String,
    pub contract_strkey: String,
    pub contract_hash: [u8; 32],
    pub function: String,
    pub args: Vec<ScVal>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetTransactionResult {
    status: String,
    tx_hash: Option<String>,
    ledger: Option<u32>,
    created_at: Option<String>,
    envelope_xdr: Option<String>,
    events: Option<TxEvents>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TxEvents {
    contract_events_xdr: Option<Vec<Vec<String>>>,
}

pub fn normalize_stellar_tx_hash(hash: &str) -> Result<String, VerifierError> {
    check_str_len("tx_hash", hash, MAX_TX_HASH_LEN).map_err(VerifierError::Failed)?;
    let trimmed = hash.trim();
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(VerifierError::Failed("invalid stellar tx hash".into()));
    }
    Ok(hex.to_ascii_lowercase())
}

impl StellarRpcClient {
    pub async fn get_finalized_transaction(
        &self,
        tx_hash: &str,
    ) -> Result<FinalizedTx, VerifierError> {
        self.ensure_url(&self.rpc_url)?;
        let normalized = normalize_stellar_tx_hash(tx_hash)?;
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTransaction",
            "params": { "hash": normalized }
        });
        let body_str = body.to_string();
        if body_str.len() > crate::cctp::stellar_rpc::MAX_JSON_BODY_BYTES {
            return Err(VerifierError::Failed("rpc request too large".into()));
        }
        let resp = self
            .client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| VerifierError::Transient(e.to_string()))?;
        let text = resp
            .text()
            .await
            .map_err(|e| VerifierError::Transient(e.to_string()))?;
        check_rpc_response_len(text.len())?;

        #[derive(Deserialize)]
        struct RpcResponse<T> {
            result: Option<T>,
            error: Option<RpcError>,
        }
        #[derive(Deserialize)]
        struct RpcError {
            message: String,
        }

        let payload: RpcResponse<GetTransactionResult> =
            serde_json::from_str(&text).map_err(|e| VerifierError::Failed(e.to_string()))?;
        if let Some(err) = payload.error {
            let lower = err.message.to_ascii_lowercase();
            if lower.contains("not found") {
                return Err(VerifierError::TxNotFound);
            }
            if lower.contains("rate") || lower.contains("timeout") {
                return Err(VerifierError::Transient(err.message));
            }
            return Err(VerifierError::Failed(err.message));
        }
        let result = payload.result.ok_or(VerifierError::TxNotFound)?;

        match result.status.as_str() {
            "NOT_FOUND" => return Err(VerifierError::TxNotFound),
            "SUCCESS" => {}
            "FAILED" => {}
            other => return Err(VerifierError::Failed(format!("unknown tx status {other}"))),
        }

        if let Some(returned) = result.tx_hash.as_deref() {
            let returned_norm = normalize_stellar_tx_hash(returned)?;
            if returned_norm != normalized {
                return Err(VerifierError::Failed("tx hash mismatch".into()));
            }
        }

        let ledger = result
            .ledger
            .ok_or_else(|| VerifierError::Failed("missing ledger".into()))?;
        self.ensure_finalized(ledger).await?;

        let envelope_xdr = result
            .envelope_xdr
            .ok_or_else(|| VerifierError::Failed("missing envelope".into()))?;
        if envelope_xdr.len() > 512 * 1024 {
            return Err(VerifierError::Failed("envelope too large".into()));
        }

        let nested = result
            .events
            .and_then(|e| e.contract_events_xdr)
            .unwrap_or_default();
        let contract_events = collect_contract_events(&nested)?;

        let status = if result.status == "SUCCESS" {
            TxStatus::Success
        } else {
            TxStatus::Failed
        };

        Ok(FinalizedTx {
            tx_hash: normalized,
            status,
            ledger,
            created_at: result.created_at,
            envelope_xdr,
            contract_events,
        })
    }

    async fn ensure_finalized(&self, tx_ledger: u32) -> Result<(), VerifierError> {
        let latest = self.latest_ledger().await?;
        if latest.saturating_sub(tx_ledger) + 1 < MIN_LEDGER_CONFIRMATIONS {
            return Err(VerifierError::Failed(
                "insufficient ledger confirmations".into(),
            ));
        }
        Ok(())
    }

    pub async fn simulate_is_nonce_used(
        &self,
        message_transmitter: &str,
        nonce: [u8; 32],
    ) -> Result<bool, VerifierError> {
        let val = self
            .simulate_scval(
                message_transmitter,
                "is_nonce_used",
                vec![ScVal::Bytes(ScBytes(
                    nonce
                        .to_vec()
                        .try_into()
                        .map_err(|_| VerifierError::Failed("nonce bytes".into()))?,
                ))],
            )
            .await?;
        crate::cctp::stellar_rpc::scval_to_bool(&val)
    }
}

pub fn ensure_testnet_binding(config: &CctpConfig) -> Result<(), VerifierError> {
    if config.stellar_network_passphrase != STELLAR_TESTNET_PASSPHRASE {
        return Err(VerifierError::Failed("wrong network passphrase".into()));
    }
    Ok(())
}

pub fn parse_invoke_envelope(envelope_xdr: &str) -> Result<ParsedInvoke, VerifierError> {
    let env = TransactionEnvelope::from_xdr_base64(envelope_xdr, Limits::none())
        .map_err(|e| VerifierError::Failed(e.to_string()))?;
    let TransactionEnvelope::Tx(TransactionV1Envelope { tx, .. }) = env else {
        return Err(VerifierError::Failed("expected v1 tx".into()));
    };
    if tx.operations.len() != 1 {
        return Err(VerifierError::Failed("expected single operation".into()));
    }
    let op = &tx.operations[0];
    let source = match &tx.source_account {
        stellar_xdr::curr::MuxedAccount::Ed25519(Uint256(bytes)) => {
            format!("{}", stellar_strkey::ed25519::PublicKey(*bytes))
        }
        _ => return Err(VerifierError::Failed("unsupported source account".into())),
    };
    let body = match &op.body {
        OperationBody::InvokeHostFunction(invoke) => invoke,
        _ => return Err(VerifierError::Failed("expected invoke".into())),
    };
    let HostFunction::InvokeContract(args) = &body.host_function else {
        return Err(VerifierError::Failed("expected contract invoke".into()));
    };
    parse_invoke_args(source, args)
}

fn parse_invoke_args(
    source: String,
    args: &InvokeContractArgs,
) -> Result<ParsedInvoke, VerifierError> {
    let contract_strkey = match &args.contract_address {
        ScAddress::Contract(hash) => format!("{}", stellar_strkey::Contract(hash.0)),
        _ => return Err(VerifierError::Failed("invoke target not contract".into())),
    };
    let function = args.function_name.0.to_string();
    Ok(ParsedInvoke {
        source_account: source,
        contract_hash: match &args.contract_address {
            ScAddress::Contract(hash) => hash.0,
            _ => return Err(VerifierError::Failed("contract hash".into())),
        },
        contract_strkey,
        function,
        args: args.args.to_vec(),
    })
}

pub fn scval_to_address(val: &ScVal) -> Result<ScAddress, VerifierError> {
    crate::cctp::stellar_contract_events::scval_to_address(val)
}

pub fn scval_to_i128(val: &ScVal) -> Result<i128, VerifierError> {
    crate::cctp::stellar_contract_events::scval_to_i128(val)
}

pub fn scval_to_u32(val: &ScVal) -> Result<u32, VerifierError> {
    crate::cctp::stellar_contract_events::scval_to_u32(val)
}

pub fn scval_to_bytes32(val: &ScVal) -> Result<[u8; 32], VerifierError> {
    crate::cctp::stellar_contract_events::scval_to_bytes32(val)
}

pub fn scval_to_bytes(val: &ScVal) -> Result<Vec<u8>, VerifierError> {
    crate::cctp::stellar_contract_events::scval_to_bytes(val)
}

pub fn contract_matches_strkey(hash: [u8; 32], strkey: &str) -> Result<bool, VerifierError> {
    let expected =
        stellar_contract_to_bytes32(strkey).map_err(|e| VerifierError::Failed(e.to_string()))?;
    Ok(hash == expected)
}

pub fn chain_id_string() -> String {
    STELLAR_TESTNET_CHAIN_ID.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_hash_with_and_without_0x() {
        let raw = "a".repeat(64);
        assert_eq!(
            normalize_stellar_tx_hash(&raw).unwrap(),
            normalize_stellar_tx_hash(&format!("0x{raw}")).unwrap()
        );
    }

    #[test]
    fn rejects_invalid_hash_lengths() {
        assert!(normalize_stellar_tx_hash("abc").is_err());
    }
}
