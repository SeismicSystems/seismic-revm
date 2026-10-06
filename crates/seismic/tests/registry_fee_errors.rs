//! Required-read failures retain their internal classification and roll back accounting.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use revm::{
    bytecode::opcode::{PUSH0, PUSH1, PUSH32, STOP},
    context::{result::EVMError, ContextTr, TxEnv},
    database::InMemoryDB,
    database_interface::DBErrorMarker,
    handler::EvmTr,
    inspector::{InspectEvm, NoOpInspector},
    primitives::{Address, Bytes, FlaggedStorage, TxKind, B256, U256},
    state::{AccountInfo, Bytecode},
    Context, Database, DatabaseCommit, ExecuteEvm,
};
use rstest::rstest;
use seismic_revm::{
    gas_token_registry::{
        balance_storage_key, token_metadata_slot, GAS_TOKEN_REGISTRY, TOKEN_COUNT_SLOT,
    },
    DefaultSeismicContext, GasPayment, SeismicBuilder, SeismicTransaction,
};

const CALLER: Address = Address::repeat_byte(0x11);
const BENEFICIARY: Address = Address::repeat_byte(0x22);
const TOKEN: Address = Address::repeat_byte(0x77);
const BODY: Address = Address::repeat_byte(0x99);

#[derive(Debug)]
struct ReadError;
impl core::fmt::Display for ReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("injected required storage read error")
    }
}
impl core::error::Error for ReadError {}
impl DBErrorMarker for ReadError {}

#[derive(Debug)]
struct FailingDb {
    inner: InMemoryDB,
    fail_at: (Address, U256),
    reads: Vec<(Address, U256)>,
}
impl Database for FailingDb {
    type Error = ReadError;
    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, ReadError> {
        Ok(self.inner.basic(address).unwrap())
    }
    fn code_by_hash(&mut self, hash: B256) -> Result<Bytecode, ReadError> {
        Ok(self.inner.code_by_hash(hash).unwrap())
    }
    fn storage(&mut self, address: Address, key: U256) -> Result<FlaggedStorage, ReadError> {
        self.reads.push((address, key));
        if self.fail_at == (address, key) {
            Err(ReadError)
        } else {
            Ok(self.inner.storage(address, key).unwrap())
        }
    }
    fn block_hash(&mut self, number: u64) -> Result<B256, ReadError> {
        Ok(self.inner.block_hash(number).unwrap())
    }
}

#[derive(Clone, Copy, Debug)]
enum Stage {
    Registry,
    Candidate,
    Reward,
    BodyBeforeMismatch,
    NativeBody,
    DisabledBody,
}

#[rstest]
fn read_errors_discard_accounting_preserve_prior_touches_and_allow_next_transaction(
    #[values(
        Stage::Registry,
        Stage::Candidate,
        Stage::Reward,
        Stage::BodyBeforeMismatch,
        Stage::NativeBody,
        Stage::DisabledBody
    )]
    stage: Stage,
    #[values(false, true)] inspected: bool,
    #[values(false, true)] pre_touched: bool,
) {
    let root = U256::from(3);
    let initial_native = U256::from(1_000_000_000_000_000_000u64);
    let initial_token = U256::from(1_000_000);
    let mut db = InMemoryDB::default();
    db.insert_account_info(
        CALLER,
        AccountInfo {
            balance: initial_native,
            ..Default::default()
        },
    );
    db.insert_account_info(
        TOKEN,
        AccountInfo {
            nonce: 1,
            ..Default::default()
        },
    );
    db.insert_account_info(
        GAS_TOKEN_REGISTRY,
        AccountInfo {
            nonce: 1,
            ..Default::default()
        },
    );
    db.insert_account_storage(GAS_TOKEN_REGISTRY, TOKEN_COUNT_SLOT, U256::from(1).into())
        .unwrap();
    let metadata = U256::from_be_slice(TOKEN.as_slice())
        | (U256::from(1) << 160usize)
        | (U256::from(1) << 168usize)
        | (U256::from(6) << 176usize);
    db.insert_account_storage(GAS_TOKEN_REGISTRY, token_metadata_slot(0), metadata.into())
        .unwrap();
    db.insert_account_storage(
        GAS_TOKEN_REGISTRY,
        token_metadata_slot(0) + U256::from(1),
        root.into(),
    )
    .unwrap();
    db.insert_account_storage(
        TOKEN,
        balance_storage_key(CALLER, root),
        initial_token.into(),
    )
    .unwrap();
    let mut to = BODY;
    let mut body = vec![STOP];
    let fail_at = match stage {
        Stage::Registry => (GAS_TOKEN_REGISTRY, TOKEN_COUNT_SLOT),
        Stage::Candidate => (TOKEN, balance_storage_key(CALLER, root)),
        Stage::Reward => (TOKEN, balance_storage_key(BENEFICIARY, root)),
        Stage::BodyBeforeMismatch => {
            to = TOKEN;
            // First write a contradictory private beneficiary slot, then fail a CSTORE read.
            body = vec![PUSH1, 1, PUSH32];
            body.extend_from_slice(&balance_storage_key(BENEFICIARY, root).to_be_bytes::<32>());
            body.extend_from_slice(&[0xb1, PUSH1, 1, PUSH1, 0, 0xb1, STOP]);
            (TOKEN, U256::ZERO)
        }
        Stage::NativeBody | Stage::DisabledBody => {
            body = vec![PUSH1, 1, PUSH0, 0xb1, STOP];
            (BODY, U256::ZERO)
        }
    };
    db.insert_account_info(
        to,
        AccountInfo::from_bytecode(Bytecode::new_raw(Bytes::from(body))),
    );
    let context = Context::seismic_with_rng_key([0; 64])
        .modify_block_chained(|block| {
            block.basefee = 0;
            block.beneficiary = BENEFICIARY;
        })
        .modify_cfg_chained(|cfg| {
            cfg.disable_balance_check = matches!(stage, Stage::DisabledBody);
        })
        .with_db(FailingDb {
            inner: db,
            fail_at,
            reads: Vec::new(),
        });
    let mut evm = context.build_seismic_evm_with_inspector(NoOpInspector);
    let mut tx = SeismicTransaction::<TxEnv>::default();
    tx.base.tx_type = 74;
    tx.base.caller = CALLER;
    tx.base.kind = TxKind::Call(Address::repeat_byte(0xaa));
    tx.base.gas_limit = 100_000;
    if pre_touched {
        let result = if inspected {
            evm.inspect_one_tx(tx.clone())
        } else {
            evm.transact_one(tx.clone())
        }
        .unwrap();
        assert!(result.is_success());
        tx.base.nonce = 1;
    }
    tx.base.kind = TxKind::Call(to);
    tx.base.gas_price = 1_000_000_000_000;
    tx.gas_payment = match stage {
        Stage::NativeBody => GasPayment::Native,
        Stage::DisabledBody => GasPayment::Token(Address::repeat_byte(0xee)), // Unknown is irrelevant with checks disabled.
        _ => GasPayment::Token(TOKEN),
    };
    let result = if inspected {
        evm.inspect_one_tx(tx.clone())
    } else {
        evm.transact_one(tx.clone())
    };
    assert!(
        matches!(result, Err(EVMError::Database(ReadError))),
        "body errors must not become mode-mismatch transaction errors: {result:?}"
    );
    assert!(evm.ctx().error().is_ok());
    assert!(evm.ctx().chain().token_fee().is_none());
    let state = &evm.ctx().journaled_state.state;
    assert_eq!(state[&CALLER].info.nonce, u64::from(pre_touched));
    assert_eq!(state[&CALLER].info.balance, initial_native);
    assert_eq!(state[&CALLER].is_touched(), pre_touched);
    assert!(!state
        .get(&TOKEN)
        .is_some_and(|account| account.is_touched()));
    if matches!(stage, Stage::NativeBody | Stage::DisabledBody) {
        assert_eq!(
            evm.ctx().db_mut().reads,
            vec![(BODY, U256::ZERO)],
            "native/disabled fee paths must not consult registry or token storage"
        );
    }
    // Reuse the retained journal after the error: no stale Context.error or fee reserve.
    tx.base.kind = TxKind::Call(Address::repeat_byte(0xaa));
    tx.base.gas_price = 0;
    tx.gas_payment = GasPayment::Auto;
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    }
    .unwrap();
    assert!(result.is_success());
    let state = evm.finalize();
    evm.ctx().db_mut().inner.commit(state);
    let db = &mut evm.ctx().db_mut().inner;
    assert_eq!(
        db.basic(CALLER).unwrap().unwrap().nonce,
        u64::from(pre_touched) + 1
    );
    assert_eq!(db.basic(CALLER).unwrap().unwrap().balance, initial_native);
    assert_eq!(
        db.storage(TOKEN, balance_storage_key(CALLER, root))
            .unwrap()
            .value,
        initial_token
    );
    assert_eq!(
        db.storage(TOKEN, balance_storage_key(BENEFICIARY, root))
            .unwrap(),
        FlaggedStorage::ZERO
    );
}
