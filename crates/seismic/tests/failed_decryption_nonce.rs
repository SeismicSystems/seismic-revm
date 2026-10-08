//! Processed transactions consume a nonce even when calldata decryption fails.
//!
//! Set the failure flag directly to exercise the handler contract used by the
//! block executor's failure adapter, not the ciphertext decryption pipeline.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use revm::{
    bytecode::opcode::{PUSH0, REVERT, STOP},
    context::{
        result::{EVMError, ExecutionResult, InvalidTransaction},
        ContextTr, TxEnv,
    },
    database::InMemoryDB,
    database_interface::DBErrorMarker,
    handler::EvmTr,
    inspector::{InspectEvm, NoOpInspector},
    primitives::{keccak256, Address, Bytes, FlaggedStorage, TxKind, B256, U256},
    state::{AccountInfo, Bytecode},
    Context, Database, ExecuteCommitEvm, ExecuteEvm,
};
use rstest::rstest;
use seismic_revm::{
    api::default_ctx::SeismicContext, handler::TOKEN, transaction::abstraction::SeismicTransaction,
    DefaultSeismicContext, SeismicBuilder,
};

const CALLER: Address = Address::repeat_byte(0x11);
const BENEFICIARY: Address = Address::repeat_byte(0x22);
const INITIAL_NONCE: u64 = 7;
const INITIAL_BALANCE: u64 = 1_000_000;

fn token_balance_slot(address: Address) -> U256 {
    // The existing gas token's balances mapping is at storage slot 3.
    let mut data = [0; 64];
    data[12..32].copy_from_slice(address.as_slice());
    data[63] = 3;
    keccak256(data).into()
}

fn funded_context(token_funded: bool) -> SeismicContext<InMemoryDB> {
    let mut db = InMemoryDB::default();
    db.insert_account_info(
        CALLER,
        AccountInfo {
            nonce: INITIAL_NONCE,
            balance: if token_funded {
                U256::ZERO
            } else {
                U256::from(INITIAL_BALANCE)
            },
            ..Default::default()
        },
    );
    db.insert_account_info(BENEFICIARY, AccountInfo::default());
    if token_funded {
        db.insert_account_info(
            TOKEN,
            AccountInfo {
                nonce: 1,
                ..Default::default()
            },
        );
        db.insert_account_storage(
            TOKEN,
            token_balance_slot(CALLER),
            U256::from(INITIAL_BALANCE).into(),
        )
        .unwrap();
    }
    Context::seismic_with_rng_key([0; 64])
        .modify_block_chained(|block| {
            block.basefee = 0;
            block.beneficiary = BENEFICIARY;
        })
        .with_db(db)
}

fn transaction(
    create: bool,
    decryption_failed: bool,
    token_funded: bool,
) -> SeismicTransaction<TxEnv> {
    let mut tx = SeismicTransaction::default();
    tx.base.tx_type = 74;
    tx.base.caller = CALLER;
    tx.base.nonce = INITIAL_NONCE;
    tx.base.gas_limit = 100_000;
    // At 10^12 wei per gas, token accounting is exact in whole token units.
    tx.base.gas_price = if token_funded { 1_000_000_000_000 } else { 3 };
    tx.base.gas_priority_fee = None;
    tx.base.value = U256::ZERO;
    tx.base.kind = if create {
        TxKind::Create
    } else {
        TxKind::Call(Address::ZERO)
    };
    tx.base.data = Bytes::from_static(&[STOP]);
    tx.decryption_failed = decryption_failed;
    tx
}

#[rstest]
fn processed_transaction_consumes_exactly_one_nonce(
    #[values(false, true)] inspected: bool,
    #[values(false, true)] create: bool,
    #[values(false, true)] decryption_failed: bool,
    #[values(false, true)] token_funded: bool,
) {
    let mut evm = funded_context(token_funded).build_seismic_evm_with_inspector(NoOpInspector);
    let tx = transaction(create, decryption_failed, token_funded);
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    }
    .unwrap();
    let state = evm.finalize();

    if decryption_failed {
        assert!(
            matches!(&result, ExecutionResult::Revert { output, .. } if output.is_empty()),
            "failed decryption must return an empty revert: {result:?}"
        );
        if create {
            assert!(
                !state
                    .get(&CALLER.create(INITIAL_NONCE))
                    .is_some_and(|account| account.is_created()),
                "failed decryption must not create a contract"
            );
        }
    } else {
        assert!(result.is_success(), "normal execution failed: {result:?}");
        if create {
            assert_eq!(
                result.created_address(),
                Some(CALLER.create(INITIAL_NONCE)),
                "ordinary creation must derive its address from the old nonce"
            );
        }
    }
    assert_eq!(
        state[&CALLER].info.nonce,
        INITIAL_NONCE + 1,
        "every processed transaction must consume exactly one sender nonce"
    );
}

#[rstest]
fn committed_decryption_failure_rejects_replay_and_accepts_next_nonce(
    #[values(false, true)] inspected: bool,
    #[values(false, true)] create: bool,
    #[values(false, true)] token_funded: bool,
) {
    let mut evm = funded_context(token_funded).build_seismic_evm_with_inspector(NoOpInspector);
    let mut tx = transaction(create, true, token_funded);
    let result = if inspected {
        evm.inspect_one_tx(tx.clone())
    } else {
        evm.transact_one(tx.clone())
    }
    .unwrap();
    assert!(matches!(result, ExecutionResult::Revert { .. }));
    let state = evm.finalize();
    evm.commit(state);

    // Check replay rejection independently of the nonce assertion above, and
    // reload committed state from the DB rather than relying on a retained journal.
    let replay = if inspected {
        evm.inspect_one_tx(tx.clone())
    } else {
        evm.transact_one(tx.clone())
    };
    assert!(
        matches!(
            &replay,
            Err(EVMError::Transaction(InvalidTransaction::NonceTooLow { tx, state }))
                if *tx == INITIAL_NONCE && *state == INITIAL_NONCE + 1
        ),
        "replaying a committed failed-decryption transaction must reject the consumed nonce: {replay:?}"
    );

    tx.base.nonce = INITIAL_NONCE + 1;
    let next = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    }
    .unwrap();
    assert!(matches!(next, ExecutionResult::Revert { .. }));
    assert_eq!(evm.finalize()[&CALLER].info.nonce, INITIAL_NONCE + 2);
}

#[rstest]
fn ordinary_initcode_revert_still_consumes_nonce(
    #[values(false, true)] inspected: bool,
    #[values(false, true)] token_funded: bool,
) {
    let mut evm = funded_context(token_funded).build_seismic_evm_with_inspector(NoOpInspector);
    let mut tx = transaction(true, false, token_funded);
    tx.base.data = Bytes::from_static(&[PUSH0, PUSH0, REVERT]);
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    }
    .unwrap();
    assert!(matches!(result, ExecutionResult::Revert { .. }));
    assert_eq!(evm.finalize()[&CALLER].info.nonce, INITIAL_NONCE + 1);
}

#[rstest]
fn max_nonce_is_rejected_before_accounting(
    #[values(false, true)] inspected: bool,
    #[values(false, true)] create: bool,
    #[values(false, true)] decryption_failed: bool,
    #[values(false, true)] token_funded: bool,
) {
    let mut ctx = funded_context(token_funded);
    let mut caller = ctx.db_mut().basic(CALLER).unwrap().unwrap();
    caller.nonce = u64::MAX;
    let original_balance = caller.balance;
    ctx.db_mut().insert_account_info(CALLER, caller);
    let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector);
    let mut tx = transaction(create, decryption_failed, token_funded);
    tx.base.nonce = u64::MAX;
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    };
    assert!(
        matches!(
            result,
            Err(EVMError::Transaction(
                InvalidTransaction::NonceOverflowInTransaction
            ))
        ),
        "nonce-max sender must be rejected before accounting: {result:?}"
    );
    let state = evm.finalize();
    assert_eq!(state[&CALLER].info.nonce, u64::MAX);
    assert_eq!(state[&CALLER].info.balance, original_balance);
    assert!(!state[&CALLER].is_touched());
    assert!(!state
        .get(&BENEFICIARY)
        .is_some_and(|account| account.is_touched()));
    if token_funded {
        // Early validation must not load or mutate token fee storage.
        assert!(!state.contains_key(&TOKEN));
        assert_eq!(
            evm.ctx()
                .db_mut()
                .storage(TOKEN, token_balance_slot(CALLER))
                .unwrap()
                .value,
            U256::from(INITIAL_BALANCE)
        );
        assert_eq!(
            evm.ctx()
                .db_mut()
                .storage(TOKEN, token_balance_slot(BENEFICIARY))
                .unwrap()
                .value,
            U256::ZERO
        );
    }
    assert!(!evm.ctx().chain().used_erc20_gas());
}

#[rstest]
fn max_minus_one_nonce_is_processed(
    #[values(false, true)] inspected: bool,
    #[values(false, true)] create: bool,
    #[values(false, true)] decryption_failed: bool,
    #[values(false, true)] token_funded: bool,
) {
    let mut ctx = funded_context(token_funded);
    let mut caller = ctx.db_mut().basic(CALLER).unwrap().unwrap();
    caller.nonce = u64::MAX - 1;
    ctx.db_mut().insert_account_info(CALLER, caller);
    let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector);
    let mut tx = transaction(create, decryption_failed, token_funded);
    tx.base.nonce = u64::MAX - 1;
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    }
    .unwrap();
    if decryption_failed {
        assert!(matches!(result, ExecutionResult::Revert { .. }));
    } else {
        assert!(result.is_success(), "normal execution failed: {result:?}");
        if create {
            assert_eq!(result.created_address(), Some(CALLER.create(u64::MAX - 1)));
        }
    }
    assert_eq!(evm.finalize()[&CALLER].info.nonce, u64::MAX);
}

#[rstest]
fn failed_decryption_create_execution_nonce_overflow_restores_accounting(
    #[values(false, true)] inspected: bool,
    #[values(false, true)] token_funded: bool,
) {
    let mut ctx = funded_context(token_funded).modify_cfg_chained(|cfg| {
        // Bypass sender validation to retain coverage of the defensive
        // checked increment in the synthetic CREATE execution branch.
        cfg.disable_nonce_check = true;
    });
    let mut caller = ctx.db_mut().basic(CALLER).unwrap().unwrap();
    caller.nonce = u64::MAX;
    let original_balance = caller.balance;
    ctx.db_mut().insert_account_info(CALLER, caller);
    let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector);
    let tx = transaction(true, true, token_funded);
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    };
    assert!(
        matches!(
            result,
            Err(EVMError::Transaction(
                InvalidTransaction::NonceOverflowInTransaction
            ))
        ),
        "execution nonce overflow must roll back accounting: {result:?}"
    );
    let state = evm.finalize();
    assert_eq!(state[&CALLER].info.nonce, u64::MAX);
    assert_eq!(state[&CALLER].info.balance, original_balance);
    if token_funded {
        let storage = &state[&TOKEN].storage;
        assert_eq!(
            storage[&token_balance_slot(CALLER)].present_value.value,
            U256::from(INITIAL_BALANCE)
        );
        assert_eq!(
            storage[&token_balance_slot(BENEFICIARY)]
                .present_value
                .value,
            U256::ZERO
        );
    }
    assert!(!evm.ctx().chain().used_erc20_gas());
}

#[derive(Debug)]
struct RewardError;

impl core::fmt::Display for RewardError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("injected beneficiary database error")
    }
}

impl core::error::Error for RewardError {}
impl DBErrorMarker for RewardError {}

#[derive(Debug)]
struct RewardErrorDb;

impl Database for RewardErrorDb {
    type Error = RewardError;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        if address == BENEFICIARY {
            // Native fee settlement loads the beneficiary after execution.
            return Err(RewardError);
        }
        Ok(if address == CALLER {
            Some(AccountInfo {
                nonce: INITIAL_NONCE,
                balance: U256::from(INITIAL_BALANCE),
                ..Default::default()
            })
        } else {
            None
        })
    }

    fn code_by_hash(&mut self, _: B256) -> Result<Bytecode, Self::Error> {
        Err(RewardError)
    }

    fn storage(&mut self, _: Address, _: U256) -> Result<FlaggedStorage, Self::Error> {
        Err(RewardError)
    }

    fn block_hash(&mut self, _: u64) -> Result<B256, Self::Error> {
        Ok(B256::ZERO)
    }
}

#[rstest]
fn transaction_error_restores_nonce_and_balance(
    #[values(false, true)] inspected: bool,
    #[values(false, true)] create: bool,
    #[values(false, true)] decryption_failed: bool,
) {
    let mut evm = Context::seismic_with_rng_key([0; 64])
        .modify_block_chained(|block| {
            block.basefee = 0;
            block.beneficiary = BENEFICIARY;
        })
        .with_db(RewardErrorDb)
        .build_seismic_evm_with_inspector(NoOpInspector);
    let tx = transaction(create, decryption_failed, false);
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    };
    assert!(
        matches!(result, Err(EVMError::Database(RewardError))),
        "expected a transaction-level settlement error: {result:?}"
    );

    assert_eq!(evm.ctx().journal().state[&CALLER].info.nonce, INITIAL_NONCE);
    assert_eq!(
        evm.ctx().journal().state[&CALLER].info.balance,
        U256::from(INITIAL_BALANCE)
    );
    let state = evm.finalize();
    assert_eq!(state[&CALLER].info.nonce, INITIAL_NONCE);
    assert_eq!(state[&CALLER].info.balance, U256::from(INITIAL_BALANCE));
}
