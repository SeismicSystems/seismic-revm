//! Authorization changes persist on bytecode reverts, but not transaction errors.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use revm::{
    context::{
        result::{EVMError, InvalidTransaction},
        ContextTr, JournalTr, TxEnv,
    },
    context_interface::transaction::{Authorization, RecoveredAuthority, RecoveredAuthorization},
    database_interface::DBErrorMarker,
    handler::{EthFrame, EvmTr, Handler},
    interpreter::interpreter::EthInterpreter,
    primitives::{Address, Bytes, FlaggedStorage, TxKind, B256, U256},
    state::{Account, AccountInfo, Bytecode},
    Context, Database, ExecuteEvm,
};
use seismic_revm::{
    handler::SeismicHandler, transaction::abstraction::SeismicTransaction, DefaultSeismicContext,
    SeismicBuilder,
};

const CALLER: Address = Address::repeat_byte(0x11);
const AUTHORITY: Address = Address::repeat_byte(0x22);
const OLD_TARGET: Address = Address::repeat_byte(0x33);
const NEW_TARGET: Address = Address::repeat_byte(0x44);
const FAILING_CONTRACT: Address = Address::repeat_byte(0x55);
const REVERTING_CONTRACT: Address = Address::repeat_byte(0x66);
const FAILING_AUTHORITY: Address = Address::repeat_byte(0x77);

#[derive(Debug)]
struct InjectedError;

impl core::fmt::Display for InjectedError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("injected database failure")
    }
}

impl core::error::Error for InjectedError {}
impl DBErrorMarker for InjectedError {}

#[derive(Debug)]
struct AuthDb {
    authority: Option<AccountInfo>,
}

impl Database for AuthDb {
    type Error = InjectedError;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        if address == FAILING_AUTHORITY {
            return Err(InjectedError);
        }
        Ok(if address == AUTHORITY {
            self.authority.clone()
        } else if address == CALLER {
            Some(AccountInfo {
                balance: U256::from(1_000_000_000u64),
                ..Default::default()
            })
        } else if address == FAILING_CONTRACT || address == REVERTING_CONTRACT {
            let bytes = if address == FAILING_CONTRACT {
                vec![0x5f, 0x54, 0x00] // PUSH0; SLOAD; STOP
            } else {
                vec![0x5f, 0x5f, 0xfd] // PUSH0; PUSH0; REVERT
            };
            let code = Bytecode::new_raw(Bytes::from(bytes));
            Some(AccountInfo {
                code_hash: code.hash_slow(),
                code: Some(code),
                nonce: 1,
                ..Default::default()
            })
        } else {
            None
        })
    }

    fn code_by_hash(&mut self, _: B256) -> Result<Bytecode, Self::Error> {
        Err(InjectedError)
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<FlaggedStorage, Self::Error> {
        assert_eq!(address, FAILING_CONTRACT);
        assert_eq!(index, U256::ZERO);
        Err(InjectedError)
    }

    fn block_hash(&mut self, _: u64) -> Result<B256, Self::Error> {
        Ok(B256::ZERO)
    }
}

fn authority_info(target: Address, nonce: u64) -> AccountInfo {
    let code = if target.is_zero() {
        Bytecode::default()
    } else {
        Bytecode::new_eip7702(target)
    };
    AccountInfo {
        nonce,
        code_hash: code.hash_slow(),
        code: Some(code),
        ..Default::default()
    }
}

fn authorization(authority: Address, target: Address, nonce: u64) -> RecoveredAuthorization {
    RecoveredAuthorization::new_unchecked(
        Authorization {
            chain_id: U256::ZERO,
            address: target,
            nonce,
        },
        RecoveredAuthority::Valid(authority),
    )
}

fn tx(
    destination: Address,
    nonce: u64,
    authorizations: &[(Address, u64)],
) -> SeismicTransaction<TxEnv> {
    let mut tx = SeismicTransaction::default();
    tx.base.tx_type = 74;
    tx.base.caller = CALLER;
    tx.base.kind = TxKind::Call(destination);
    tx.base.nonce = nonce;
    tx.base.gas_limit = 200_000;
    tx.base.gas_price = 0;
    tx.base.gas_priority_fee = None;
    tx.base.set_recovered_authorization(
        authorizations
            .iter()
            .map(|&(target, nonce)| authorization(AUTHORITY, target, nonce))
            .collect(),
    );
    tx
}

fn assert_authority(account: &Account, expected: &AccountInfo, touched: bool) {
    assert_eq!(account.info.nonce, expected.nonce);
    assert_eq!(account.info.balance, expected.balance);
    assert_eq!(account.info.code_hash, expected.code_hash);
    assert_eq!(account.info.code, expected.code);
    assert_eq!(account.is_touched(), touched);
}

#[test]
fn transaction_error_restores_replaced_cleared_and_new_delegations() {
    for initial_target in [OLD_TARGET, Address::ZERO] {
        for new_target in [NEW_TARGET, Address::ZERO] {
            for tx_type in [4, 74] {
                let initial = authority_info(initial_target, 5);
                let mut evm = Context::seismic_with_rng_key([0; 64])
                    .with_db(AuthDb {
                        authority: Some(initial.clone()),
                    })
                    .build_seismic_evm();
                let mut transaction = tx(FAILING_CONTRACT, 0, &[(new_target, 5)]);
                transaction.base.tx_type = tx_type;
                let result = evm.transact_one(transaction);
                assert!(
                    matches!(result, Err(EVMError::Database(InjectedError))),
                    "{result:?}"
                );
                assert_authority(&evm.ctx().journal().state[&AUTHORITY], &initial, false);
                let state = evm.finalize();
                assert_authority(&state[&AUTHORITY], &initial, false);
            }
        }
    }
}

#[test]
fn transaction_error_restores_nonexistent_authority() {
    let mut evm = Context::seismic_with_rng_key([0; 64])
        .with_db(AuthDb { authority: None })
        .build_seismic_evm();
    let result = evm.transact_one(tx(FAILING_CONTRACT, 0, &[(NEW_TARGET, 0)]));
    assert!(matches!(result, Err(EVMError::Database(InjectedError))));
    let state = evm.finalize();
    assert_authority(&state[&AUTHORITY], &authority_info(Address::ZERO, 0), false);
    assert!(state[&AUTHORITY].is_loaded_as_not_existing_not_touched());
}

#[test]
fn repeated_authorizations_rollback_in_reverse_order() {
    let initial = authority_info(OLD_TARGET, 5);
    let mut evm = Context::seismic_with_rng_key([0; 64])
        .with_db(AuthDb {
            authority: Some(initial.clone()),
        })
        .build_seismic_evm();
    let result = evm.transact_one(tx(
        FAILING_CONTRACT,
        0,
        &[(NEW_TARGET, 5), (Address::ZERO, 6), (OLD_TARGET, 7)],
    ));
    assert!(matches!(result, Err(EVMError::Database(InjectedError))));
    // Cleanup is idempotent and finalization must not revive failed changes.
    evm.ctx().journal_mut().discard_tx();
    let state = evm.finalize();
    assert_authority(&state[&AUTHORITY], &initial, false);
}

#[test]
fn rollback_preserves_previously_touched_authority() {
    let initial = authority_info(OLD_TARGET, 5);
    let mut evm = Context::seismic_with_rng_key([0; 64])
        .with_db(AuthDb {
            authority: Some(initial.clone()),
        })
        .build_seismic_evm();
    evm.ctx()
        .journal_mut()
        .load_account_code(AUTHORITY)
        .unwrap();
    evm.ctx().journal_mut().touch_account(AUTHORITY);
    evm.ctx().journal_mut().commit_tx();
    let result = evm.transact_one(tx(FAILING_CONTRACT, 0, &[(Address::ZERO, 5)]));
    assert!(matches!(result, Err(EVMError::Database(InjectedError))));
    let state = evm.finalize();
    assert_authority(&state[&AUTHORITY], &initial, true);
}

#[test]
fn failed_transaction_preserves_prior_success_in_retained_journal() {
    for retry in [false, true] {
        let mut evm = Context::seismic_with_rng_key([0; 64])
            .with_db(AuthDb {
                authority: Some(authority_info(OLD_TARGET, 5)),
            })
            .build_seismic_evm();
        assert!(evm
            .transact_one(tx(Address::ZERO, 0, &[(NEW_TARGET, 5)]))
            .unwrap()
            .is_success());
        let result = evm.transact_one(tx(FAILING_CONTRACT, 1, &[(Address::ZERO, 6)]));
        assert!(matches!(result, Err(EVMError::Database(InjectedError))));
        let mut expected = authority_info(NEW_TARGET, 6);
        assert_authority(&evm.ctx().journal().state[&AUTHORITY], &expected, true);
        if retry {
            // A subsequent valid authorization must still accept the restored nonce.
            assert!(evm
                .transact_one(tx(Address::ZERO, 1, &[(OLD_TARGET, 6)]))
                .unwrap()
                .is_success());
            expected = authority_info(OLD_TARGET, 7);
        }
        let state = evm.finalize();
        assert_authority(&state[&AUTHORITY], &expected, true);
    }
}

#[test]
fn later_authority_load_error_rolls_back_earlier_authorizations() {
    let initial = authority_info(OLD_TARGET, 5);
    let mut evm = Context::seismic_with_rng_key([0; 64])
        .with_db(AuthDb {
            authority: Some(initial.clone()),
        })
        .build_seismic_evm();
    let mut transaction = tx(Address::ZERO, 0, &[]);
    transaction.base.set_recovered_authorization(vec![
        authorization(AUTHORITY, NEW_TARGET, 5),
        authorization(FAILING_AUTHORITY, NEW_TARGET, 0),
    ]);
    let result = evm.transact_one(transaction);
    assert!(matches!(result, Err(EVMError::Database(InjectedError))));
    let state = evm.finalize();
    assert_authority(&state[&AUTHORITY], &initial, false);
}

#[test]
fn settlement_validation_error_rolls_back_authorizations() {
    let initial = authority_info(OLD_TARGET, 5);
    let mut evm = Context::seismic_with_rng_key([0; 64])
        .with_db(AuthDb {
            authority: Some(initial.clone()),
        })
        .with_tx(tx(Address::ZERO, 0, &[(NEW_TARGET, 5)]))
        .build_seismic_evm();
    let handler =
        SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
    handler.pre_execution(&mut evm).unwrap();
    assert_authority(
        &evm.ctx().journal().state[&AUTHORITY],
        &authority_info(NEW_TARGET, 6),
        true,
    );
    // Model a deterministic validation error raised after authorization processing.
    let result = handler.catch_error(
        &mut evm,
        EVMError::Transaction(InvalidTransaction::OverflowPaymentInTransaction),
    );
    assert!(matches!(
        result,
        Err(EVMError::Transaction(
            InvalidTransaction::OverflowPaymentInTransaction
        ))
    ));
    let state = evm.finalize();
    assert_authority(&state[&AUTHORITY], &initial, false);
}

#[test]
fn ordinary_bytecode_revert_keeps_authorizations() {
    for target in [NEW_TARGET, Address::ZERO] {
        let mut evm = Context::seismic_with_rng_key([0; 64])
            .with_db(AuthDb {
                authority: Some(authority_info(OLD_TARGET, 5)),
            })
            .build_seismic_evm();
        let result = evm
            .transact_one(tx(REVERTING_CONTRACT, 0, &[(target, 5)]))
            .unwrap();
        assert!(matches!(
            result,
            revm::context::result::ExecutionResult::Revert { .. }
        ));
        let state = evm.finalize();
        assert_authority(&state[&AUTHORITY], &authority_info(target, 6), true);
    }
}
