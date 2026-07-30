//! Shared Soroban invoke XDR encoding (no RPC, no simulation).

use chrono::Utc;
use stellar_xdr::curr::{
    AccountId, Hash, HostFunction, InvokeContractArgs, InvokeHostFunctionOp, Limits, Memo,
    MuxedAccount, Operation, OperationBody, Preconditions, PublicKey, ScAddress, ScBytes, ScSymbol,
    ScVal, SequenceNumber, TimeBounds, TimePoint, Transaction, TransactionEnvelope, TransactionExt,
    TransactionV1Envelope, Uint256, VecM, WriteXdr,
};

use crate::cctp::builders::BuilderError;
use crate::cctp::config::FINALITY_STANDARD;
use crate::cctp::expectations::ANY_DESTINATION_CALLER;
use crate::swap::tx::{DEFAULT_BASE_FEE, DEFAULT_TIMEOUT_SECS};

pub fn contract_address(strkey: &str) -> Result<ScAddress, BuilderError> {
    let contract = stellar_strkey::Contract::from_string(strkey.trim())
        .map_err(|_| BuilderError::Validation(format!("invalid contract: {strkey}")))?;
    Ok(ScAddress::Contract(Hash(contract.0)))
}

pub fn account_address(g: &str) -> Result<ScAddress, BuilderError> {
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

pub fn approve_args(spender_contract: &str, amount: i128) -> Result<Vec<ScVal>, BuilderError> {
    Ok(vec![
        ScVal::Address(contract_address(spender_contract)?),
        i128_scval(amount),
    ])
}

pub fn deposit_for_burn_args(
    caller: &str,
    amount_stellar: i128,
    destination_domain: u32,
    mint_recipient: [u8; 32],
    burn_token: &str,
    max_fee_stellar: i128,
) -> Result<Vec<ScVal>, BuilderError> {
    Ok(vec![
        ScVal::Address(account_address(caller)?),
        i128_scval(amount_stellar),
        u32_scval(destination_domain),
        bytes32_scval(mint_recipient),
        ScVal::Address(contract_address(burn_token)?),
        bytes32_scval(ANY_DESTINATION_CALLER),
        i128_scval(max_fee_stellar),
        u32_scval(FINALITY_STANDARD),
    ])
}

pub fn mint_and_forward_args(
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

/// Encode unsigned invoke envelope at explicit ledger sequence (sequence+1).
pub fn encode_invoke_at_sequence(
    source: &str,
    contract: &str,
    function: &str,
    args: Vec<ScVal>,
    ledger_sequence: i64,
) -> Result<String, BuilderError> {
    let invoke = InvokeHostFunctionOp {
        host_function: HostFunction::InvokeContract(InvokeContractArgs {
            contract_address: contract_address(contract)?,
            function_name: sc_symbol(function)?,
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
        seq_num: SequenceNumber(ledger_sequence + 1),
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

    envelope
        .to_xdr_base64(Limits::none())
        .map_err(|e| BuilderError::Encoding(e.to_string()))
}

pub fn envelope_sequence(xdr: &str) -> Result<i64, BuilderError> {
    use stellar_xdr::curr::ReadXdr;
    let env = TransactionEnvelope::from_xdr_base64(xdr, Limits::none())
        .map_err(|e| BuilderError::Encoding(e.to_string()))?;
    let TransactionEnvelope::Tx(v1) = env else {
        return Err(BuilderError::Encoding("expected v1 envelope".into()));
    };
    Ok(v1.tx.seq_num.0 - 1)
}
