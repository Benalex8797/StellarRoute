//! Stellar Testnet Soroban unsigned CCTP transaction builders.
//!
//! Function signatures from circlefin/stellar-cctp `cctp-interfaces/src/token_messenger.rs`
//! and Circle Stellar contracts reference (`mint_and_forward`).

use async_trait::async_trait;
use chrono::Utc;
use sha2::{Digest, Sha256};
use stellar_xdr::curr::{
    AccountId, Hash, HostFunction, InvokeContractArgs, InvokeHostFunctionOp, Limits, Memo,
    MuxedAccount, Operation, OperationBody, Preconditions, PublicKey, ScAddress, ScBytes, ScSymbol,
    ScVal, SequenceNumber, TimeBounds, TimePoint, Transaction,
    TransactionEnvelope, TransactionExt, TransactionV1Envelope, Uint256, VecM, WriteXdr,
};

use crate::cctp::builders::{
    BuilderError, PreparedBurnBundle, PreparedMintBundle, StellarCctpBurnBuilder,
    StellarCctpMintBuilder,
};
use crate::cctp::config::{CctpConfig, FINALITY_STANDARD, STELLAR_TESTNET_PASSPHRASE};
use crate::cctp::encoding::{
    cctp_subunits_to_stellar_subunits, evm_address_to_bytes32,
    stellar_outbound_cctp_amount,
};
use crate::cctp::expectations::ANY_DESTINATION_CALLER;
use crate::cctp::store::CctpTransfer;
use crate::models::v2_cctp::{CctpDirection, PreparedWalletPayload};
use crate::simulation::{SimulationConfig, SorobanSimulator};
use crate::swap::tx::{AccountSequenceSource, DEFAULT_BASE_FEE, DEFAULT_TIMEOUT_SECS};

pub struct ProductionStellarCctpBuilder {
    pub sequences: std::sync::Arc<dyn AccountSequenceSource>,
    pub simulator: Option<std::sync::Arc<SorobanSimulator>>,
}

impl ProductionStellarCctpBuilder {
    pub fn from_env(sequences: std::sync::Arc<dyn AccountSequenceSource>) -> Self {
        let simulator = std::env::var("SOROBAN_RPC_URL").ok().and_then(|url| {
            SorobanSimulator::new(SimulationConfig {
                rpc_url: url,
                ..Default::default()
            })
        });
        Self {
            sequences,
            simulator,
        }
    }

    fn ensure_not_expired(transfer: &CctpTransfer) -> Result<(), BuilderError> {
        if Utc::now() > transfer.quote_expires_at {
            return Err(BuilderError::QuoteExpired);
        }
        if let Some(fee_exp) = transfer.fee_expires_at {
            if Utc::now() > fee_exp {
                return Err(BuilderError::FeeExpired);
            }
        }
        Ok(())
    }

    fn contract_address(strkey: &str) -> Result<ScAddress, BuilderError> {
        let contract = stellar_strkey::Contract::from_string(strkey.trim())
            .map_err(|_| BuilderError::Validation(format!("invalid contract: {strkey}")))?;
        Ok(ScAddress::Contract(Hash(contract.0)))
    }

    fn account_address(g: &str) -> Result<ScAddress, BuilderError> {
        let pk = stellar_strkey::ed25519::PublicKey::from_string(g.trim())
            .map_err(|_| BuilderError::Validation(format!("invalid G-address: {g}")))?;
        let account_id = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(pk.0)));
        Ok(ScAddress::Account(account_id))
    }

    fn sc_symbol(name: &str) -> Result<ScSymbol, BuilderError> {
        ScSymbol::try_from(name.to_string())
            .map_err(|_| BuilderError::Encoding(format!("invalid symbol: {name}")))
    }

    fn bytes32_scval(bytes: [u8; 32]) -> ScVal {
        ScVal::Bytes(ScBytes(
            bytes
                .to_vec()
                .try_into()
                .unwrap_or_else(|_| panic!("bytes32 length")),
        ))
    }

    fn i128_scval(v: i128) -> ScVal {
        ScVal::I128(stellar_xdr::curr::Int128Parts {
            hi: (v >> 64) as i64,
            lo: v as u64,
        })
    }

    fn u32_scval(v: u32) -> ScVal {
        ScVal::U32(v)
    }

    /// Circle TokenMessenger `deposit_for_burn` (no hook) for Stellar->EVM.
    fn deposit_for_burn_args(
        caller: &str,
        amount_stellar: i128,
        destination_domain: u32,
        mint_recipient: [u8; 32],
        burn_token: &str,
        max_fee_stellar: i128,
    ) -> Result<Vec<ScVal>, BuilderError> {
        Ok(vec![
            ScVal::Address(Self::account_address(caller)?),
            Self::i128_scval(amount_stellar),
            Self::u32_scval(destination_domain),
            Self::bytes32_scval(mint_recipient),
            ScVal::Address(Self::contract_address(burn_token)?),
            Self::bytes32_scval(ANY_DESTINATION_CALLER),
            Self::i128_scval(max_fee_stellar),
            Self::u32_scval(FINALITY_STANDARD),
        ])
    }

    fn approve_args(spender_contract: &str, amount: i128) -> Result<Vec<ScVal>, BuilderError> {
        Ok(vec![
            ScVal::Address(Self::contract_address(spender_contract)?),
            Self::i128_scval(amount),
        ])
    }

    fn mint_and_forward_args(
        message: &[u8],
        attestation: &[u8],
    ) -> Result<Vec<ScVal>, BuilderError> {
        Ok(vec![
            ScVal::Bytes(ScBytes(
                message
                    .to_vec()
                    .try_into()
                    .map_err(|_| BuilderError::Encoding("message too large".into()))?,
            )),
            ScVal::Bytes(ScBytes(attestation.to_vec().try_into().map_err(|_| {
                BuilderError::Encoding("attestation too large".into())
            })?)),
        ])
    }

    async fn build_invoke_envelope(
        &self,
        source: &str,
        contract: &str,
        function: &str,
        args: Vec<ScVal>,
        config: &CctpConfig,
    ) -> Result<String, BuilderError> {
        let sequence = self
            .sequences
            .current_sequence(source)
            .await
            .map_err(|e| BuilderError::AccountLookup(e.to_string()))?;

        let invoke = InvokeHostFunctionOp {
            host_function: HostFunction::InvokeContract(InvokeContractArgs {
                contract_address: Self::contract_address(contract)?,
                function_name: Self::sc_symbol(function)?,
                args: args
                    .try_into()
                    .map_err(|_| BuilderError::Encoding("too many contract args".into()))?,
            }),
            auth: VecM::default(),
        };

        let now = Utc::now().timestamp() as u64;
        let tx = Transaction {
            source_account: MuxedAccount::Ed25519(Uint256(
                stellar_strkey::ed25519::PublicKey::from_string(source)
                    .map_err(|_| BuilderError::Validation("invalid source".into()))?
                    .0,
            )),
            fee: DEFAULT_BASE_FEE,
            seq_num: SequenceNumber(sequence + 1),
            cond: Preconditions::Time(TimeBounds {
                min_time: TimePoint(0),
                max_time: TimePoint(now + DEFAULT_TIMEOUT_SECS),
            }),
            memo: Memo::None,
            operations: vec![Operation {
                source_account: None,
                body: OperationBody::InvokeHostFunction(invoke),
            }]
            .try_into()
            .map_err(|_| BuilderError::Encoding("operation vec".into()))?,
            ext: TransactionExt::V0,
        };

        let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
            tx,
            signatures: VecM::default(),
        });

        let xdr = envelope
            .to_xdr_base64(Limits::none())
            .map_err(|e| BuilderError::Encoding(e.to_string()))?;

        if let Some(sim) = &self.simulator {
            let result = sim.simulate(&xdr).await;
            if result.simulated && !result.success {
                return Err(BuilderError::SimulationFailed(
                    result
                        .failure_reason
                        .unwrap_or_else(|| "simulation failed".into()),
                ));
            }
        }

        let passphrase = if config.stellar_network_passphrase.is_empty() {
            STELLAR_TESTNET_PASSPHRASE
        } else {
            &config.stellar_network_passphrase
        };

        if passphrase != STELLAR_TESTNET_PASSPHRASE {
            return Err(BuilderError::Validation("wrong network passphrase".into()));
        }

        Ok(xdr)
    }

    fn stellar_payload(xdr: String, passphrase: &str) -> PreparedWalletPayload {
        PreparedWalletPayload::StellarXdr {
            network_passphrase: passphrase.to_string(),
            xdr_envelope: xdr,
        }
    }
}

#[async_trait]
impl StellarCctpBurnBuilder for ProductionStellarCctpBuilder {
    fn is_ready(&self) -> bool {
        true
    }

    async fn prepare_burn(
        &self,
        transfer: &CctpTransfer,
        config: &CctpConfig,
    ) -> Result<PreparedBurnBundle, BuilderError> {
        if transfer.direction != CctpDirection::StellarToEvm {
            return Err(BuilderError::Validation(
                "Stellar burn builder only supports stellar_to_evm".into(),
            ));
        }
        Self::ensure_not_expired(transfer)?;
        if transfer.sender.is_empty() {
            return Err(BuilderError::Validation(
                "sender required for Stellar burn".into(),
            ));
        }

        let (cctp_amount, _) = stellar_outbound_cctp_amount(&transfer.amount)
            .map_err(|e| BuilderError::Encoding(e.to_string()))?;
        let stellar_amount = cctp_subunits_to_stellar_subunits(cctp_amount)
            .map_err(|e| BuilderError::Encoding(e.to_string()))?
            as i128;
        let max_fee = transfer
            .max_fee
            .as_deref()
            .ok_or_else(|| BuilderError::Validation("max_fee missing".into()))?;
        let max_fee_stellar = cctp_subunits_to_stellar_subunits(
            crate::cctp::encoding::decimal_to_cctp_subunits(max_fee)
                .map_err(|e| BuilderError::Encoding(e.to_string()))?,
        )
        .map_err(|e| BuilderError::Encoding(e.to_string()))? as i128;

        let mint_recipient = evm_address_to_bytes32(&transfer.recipient)
            .map_err(|e| BuilderError::Encoding(e.to_string()))?;

        let approve_args =
            Self::approve_args(&config.contracts.stellar_token_messenger, stellar_amount)?;
        let approve_xdr = self
            .build_invoke_envelope(
                &transfer.sender,
                &config.contracts.stellar_usdc,
                "approve",
                approve_args,
                config,
            )
            .await?;

        let burn_args = Self::deposit_for_burn_args(
            &transfer.sender,
            stellar_amount,
            config.sepolia_domain,
            mint_recipient,
            &config.contracts.stellar_usdc,
            max_fee_stellar,
        )?;
        let burn_xdr = self
            .build_invoke_envelope(
                &transfer.sender,
                &config.contracts.stellar_token_messenger,
                "deposit_for_burn",
                burn_args,
                config,
            )
            .await?;

        let passphrase = config.stellar_network_passphrase.clone();
        Ok(PreparedBurnBundle {
            required_approvals: vec![Self::stellar_payload(approve_xdr, &passphrase)],
            required_prior_payloads: vec![],
            primary: Self::stellar_payload(burn_xdr, &passphrase),
            expires_at: transfer.quote_expires_at.timestamp(),
        })
    }
}

#[async_trait]
impl StellarCctpMintBuilder for ProductionStellarCctpBuilder {
    fn is_ready(&self) -> bool {
        true
    }

    async fn prepare_mint(
        &self,
        transfer: &CctpTransfer,
        config: &CctpConfig,
    ) -> Result<PreparedMintBundle, BuilderError> {
        if transfer.direction != CctpDirection::EvmToStellar {
            return Err(BuilderError::Validation(
                "Stellar mint builder only supports evm_to_stellar destination".into(),
            ));
        }
        let message = transfer
            .raw_message
            .as_ref()
            .ok_or_else(|| BuilderError::Validation("raw_message missing".into()))?;
        let attestation = transfer
            .attestation
            .as_ref()
            .ok_or_else(|| BuilderError::Validation("attestation missing".into()))?;

        let sender = if transfer.recipient.is_empty() {
            return Err(BuilderError::Validation("recipient required".into()));
        } else {
            transfer.recipient.clone()
        };

        let args = Self::mint_and_forward_args(message, attestation)?;
        let xdr = self
            .build_invoke_envelope(
                &sender,
                &config.contracts.stellar_cctp_forwarder,
                "mint_and_forward",
                args,
                config,
            )
            .await?;

        let payload = Self::stellar_payload(xdr, &config.stellar_network_passphrase);
        let json = serde_json::to_string(&payload).unwrap_or_default();
        let payload_hash = hex::encode(Sha256::digest(json.as_bytes()));
        let expires_at = (Utc::now() + chrono::Duration::minutes(10)).timestamp();

        Ok(PreparedMintBundle {
            primary: payload,
            expires_at,
            payload_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cctp::config::CctpConfig;
    use crate::swap::tx::FixedAccountSequences;
    use chrono::Duration;
    use stellar_xdr::curr::ReadXdr;
    use uuid::Uuid;

    fn sample_stellar_burn_transfer() -> CctpTransfer {
        let now = Utc::now();
        CctpTransfer {
            transfer_id: Uuid::new_v4(),
            support_reference_id: "sup".into(),
            corridor_id: "c".into(),
            provider: "circle-cctp".into(),
            direction: CctpDirection::StellarToEvm,
            source_chain_id: "stellar:testnet".into(),
            destination_chain_id: "eip155:11155111".into(),
            source_asset: "a".into(),
            source_asset_canonical: "a".into(),
            destination_asset: "b".into(),
            destination_asset_canonical: "b".into(),
            sender: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF".into(),
            recipient: "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0".into(),
            amount: "1.0000000".into(),
            destination_amount: "1.0000000".into(),
            finality: crate::models::v2_cctp::CctpFinality::Standard,
            runtime_fee_quote: Some("1".into()),
            max_fee: Some("1".into()),
            fee_expires_at: Some(now + Duration::minutes(10)),
            quote_expires_at: now + Duration::minutes(10),
            status: crate::models::v2_cctp::CctpTransferStatus::BurnPrepared,
            source_tx_hash: None,
            destination_tx_hash: None,
            iris_message_hash: None,
            message_nonce: None,
            raw_message: None,
            attestation: None,
            retry_count: 0,
            last_provider_error: None,
            last_provider_code: None,
            version: 1,
            created_at: now,
            updated_at: now,
            terminal_at: None,
            mint_payload_hash: None,
            mint_payload_expires_at: None,
        }
    }

    #[tokio::test]
    async fn builds_stellar_burn_xdr_with_invoke_host_function() {
        let builder = ProductionStellarCctpBuilder {
            sequences: std::sync::Arc::new(FixedAccountSequences::new(100)),
            simulator: None,
        };
        let cfg = CctpConfig::default_testnet();
        let bundle = builder
            .prepare_burn(&sample_stellar_burn_transfer(), &cfg)
            .await
            .unwrap();
        let PreparedWalletPayload::StellarXdr { xdr_envelope, .. } = &bundle.primary else {
            panic!("expected stellar xdr");
        };
        let env = TransactionEnvelope::from_xdr_base64(xdr_envelope, Limits::none()).unwrap();
        let TransactionEnvelope::Tx(v1) = env else {
            panic!("expected v1");
        };
        assert_eq!(v1.tx.operations.len(), 1);
        assert!(matches!(
            v1.tx.operations[0].body,
            OperationBody::InvokeHostFunction(_)
        ));
        assert!(!bundle.required_approvals.is_empty());
    }
}
