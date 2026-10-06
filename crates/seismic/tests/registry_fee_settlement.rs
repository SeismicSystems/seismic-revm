//! Real execution/commit regressions for registry-based fee reserves and visibility.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use revm::{
    bytecode::opcode::{PUSH0, PUSH1, PUSH32, REVERT, SSTORE, STOP},
    context::{
        result::{EVMError, InvalidTransaction},
        ContextTr, TxEnv,
    },
    database::InMemoryDB,
    handler::EvmTr,
    inspector::{InspectEvm, NoOpInspector},
    primitives::{Address, Bytes, FlaggedStorage, TxKind, U256},
    state::{AccountInfo, Bytecode},
    Context, Database, DatabaseCommit, ExecuteEvm,
};
use rstest::rstest;
use seismic_revm::{
    api::default_ctx::SeismicContext,
    gas_token_registry::{
        balance_storage_key, token_metadata_slot, BalanceStorageMode, TokenPrecision,
        GAS_TOKEN_REGISTRY, TOKEN_COUNT_SLOT,
    },
    DefaultSeismicContext, GasPayment, SeismicBuilder, SeismicTransaction,
};

const CALLER: Address = Address::repeat_byte(0x11);
const BENEFICIARY: Address = Address::repeat_byte(0x22);
const TOKEN: Address = Address::repeat_byte(0x77);
const BODY: Address = Address::repeat_byte(0x99);
const GAS_LIMIT: u64 = 100_000;
const PRICE: u128 = 1_000_000_000_000;
const ROOT: U256 = U256::from_limbs([3, 0, 0, 0x1234]);

fn metadata(mode: BalanceStorageMode, decimals: u8, active: bool) -> U256 {
    U256::from_be_slice(TOKEN.as_slice())
        | (U256::from(u8::from(active)) << 160usize)
        | (U256::from(u8::from(mode == BalanceStorageMode::Public)) << 168usize)
        | (U256::from(decimals) << 176usize)
}

fn context(
    decimals: u8,
    mode: BalanceStorageMode,
    beneficiary: Address,
) -> (SeismicContext<InMemoryDB>, U256) {
    let precision = TokenPrecision::new(decimals).unwrap();
    let initial = precision.ceil(U256::from(GAS_LIMIT) * U256::from(PRICE)) + U256::from(100);
    let mut db = InMemoryDB::default();
    db.insert_account_info(CALLER, AccountInfo::default());
    for address in [TOKEN, GAS_TOKEN_REGISTRY, BODY] {
        db.insert_account_info(
            address,
            AccountInfo {
                nonce: 1,
                ..Default::default()
            },
        );
    }
    db.insert_account_storage(GAS_TOKEN_REGISTRY, TOKEN_COUNT_SLOT, U256::from(1).into())
        .unwrap();
    db.insert_account_storage(
        GAS_TOKEN_REGISTRY,
        token_metadata_slot(0),
        metadata(mode, decimals, true).into(),
    )
    .unwrap();
    db.insert_account_storage(
        GAS_TOKEN_REGISTRY,
        token_metadata_slot(0) + U256::from(1),
        ROOT.into(),
    )
    .unwrap();
    // The test backing state represents already-initialized balances, not value-only genesis.
    db.insert_account_storage(
        TOKEN,
        balance_storage_key(CALLER, ROOT),
        FlaggedStorage::new(initial, mode.is_private()),
    )
    .unwrap();
    (
        Context::seismic_with_rng_key([0; 64])
            .modify_block_chained(|block| {
                block.basefee = 0;
                block.beneficiary = beneficiary;
            })
            .with_db(db),
        initial,
    )
}

fn transaction() -> SeismicTransaction<TxEnv> {
    let mut tx = SeismicTransaction::default();
    tx.base.tx_type = 74;
    tx.base.caller = CALLER;
    tx.base.kind = TxKind::Call(BODY);
    tx.base.gas_limit = GAS_LIMIT;
    tx.base.gas_price = PRICE;
    tx.gas_payment = GasPayment::Token(TOKEN);
    tx
}

fn set_code(ctx: &mut SeismicContext<InMemoryDB>, address: Address, code: Vec<u8>) {
    ctx.db_mut().insert_account_info(
        address,
        AccountInfo::from_bytecode(Bytecode::new_raw(Bytes::from(code))),
    );
}

fn store_code(key: U256, value: U256, opcode: u8) -> Vec<u8> {
    let mut code = vec![PUSH32];
    code.extend_from_slice(&value.to_be_bytes::<32>());
    code.push(PUSH32);
    code.extend_from_slice(&key.to_be_bytes::<32>());
    code.extend_from_slice(&[opcode, STOP]);
    code
}

#[rstest]
fn settlement_conserves_and_commits_for_every_precision_and_mode(
    #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18)] decimals: u8,
    #[values(BalanceStorageMode::Public, BalanceStorageMode::Shielded)] mode: BalanceStorageMode,
    #[values(false, true)] inspected: bool,
    #[values(false, true)] revert: bool,
    #[values(false, true)] same_beneficiary: bool,
) {
    let beneficiary = if same_beneficiary {
        CALLER
    } else {
        BENEFICIARY
    };
    let (mut ctx, initial) = context(decimals, mode, beneficiary);
    if revert {
        set_code(&mut ctx, BODY, vec![PUSH0, PUSH0, REVERT]);
    }
    let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector);
    let result = if inspected {
        evm.inspect_one_tx(transaction())
    } else {
        evm.transact_one(transaction())
    }
    .unwrap();
    assert_eq!(result.is_success(), !revert);
    let precision = TokenPrecision::new(decimals).unwrap();
    let upfront = precision.ceil(U256::from(GAS_LIMIT) * U256::from(PRICE));
    let refund = precision.floor(U256::from(GAS_LIMIT - result.gas_used()) * U256::from(PRICE));
    let reward = upfront - refund;
    assert!(evm.ctx().chain().token_fee().is_none());
    assert!(evm.ctx().error().is_ok());
    let state = evm.finalize();
    let token = &state[&TOKEN];
    assert!(token.is_touched());
    assert!(
        token.is_cold_transaction_id(0),
        "fee accounting must not warm the token account"
    );
    assert!(token
        .storage
        .values()
        .all(|slot| slot.is_cold_transaction_id(0)));
    assert!(!state[&GAS_TOKEN_REGISTRY].is_touched());
    assert!(state[&GAS_TOKEN_REGISTRY].is_cold_transaction_id(0));
    assert_eq!(state[&CALLER].info.nonce, 1);
    evm.ctx().db_mut().commit(state);
    let caller = evm
        .ctx()
        .db_mut()
        .storage(TOKEN, balance_storage_key(CALLER, ROOT))
        .unwrap();
    assert_eq!(
        caller,
        FlaggedStorage::new(
            if same_beneficiary {
                initial
            } else {
                initial - reward
            },
            mode.is_private()
        )
    );
    if !same_beneficiary {
        let credited = evm
            .ctx()
            .db_mut()
            .storage(TOKEN, balance_storage_key(beneficiary, ROOT))
            .unwrap();
        // No initialization for a skipped zero reward (possible with 18-decimal free execution).
        assert_eq!(credited.value, reward);
        assert_eq!(credited.is_private, mode.is_private() && !reward.is_zero());
    }
}

#[rstest]
fn burning_beneficiary_balance_during_execution_cannot_spend_the_reserve(
    #[values(false, true)] inspected: bool,
    #[values(BalanceStorageMode::Public, BalanceStorageMode::Shielded)] mode: BalanceStorageMode,
) {
    let (mut ctx, initial) = context(6, mode, BENEFICIARY);
    let key = balance_storage_key(BENEFICIARY, ROOT);
    ctx.db_mut()
        .insert_account_storage(
            TOKEN,
            key,
            FlaggedStorage::new(U256::from(500), mode.is_private()),
        )
        .unwrap();
    set_code(
        &mut ctx,
        TOKEN,
        store_code(
            key,
            U256::ZERO,
            if mode.is_private() { 0xb1 } else { SSTORE },
        ),
    );
    let mut tx = transaction();
    tx.base.kind = TxKind::Call(TOKEN);
    let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector);
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    }
    .unwrap();
    assert!(result.is_success());
    let precision = TokenPrecision::new(6).unwrap();
    let net = precision.ceil(U256::from(GAS_LIMIT) * U256::from(PRICE))
        - precision.floor(U256::from(GAS_LIMIT - result.gas_used()) * U256::from(PRICE));
    let state = evm.finalize();
    evm.ctx().db_mut().commit(state);
    assert_eq!(evm.ctx().db_mut().storage(TOKEN, key).unwrap().value, net);
    assert_eq!(
        evm.ctx()
            .db_mut()
            .storage(TOKEN, balance_storage_key(CALLER, ROOT))
            .unwrap()
            .value,
        initial - net
    );
}

#[rstest]
fn registry_changes_apply_to_next_transaction_but_not_selected_settlement(
    #[values(false, true)] inspected: bool,
) {
    let (mut ctx, initial) = context(6, BalanceStorageMode::Public, BENEFICIARY);
    // Change precision and deactivate within the body. Settlement must retain six decimals.
    set_code(
        &mut ctx,
        GAS_TOKEN_REGISTRY,
        store_code(
            token_metadata_slot(0),
            metadata(BalanceStorageMode::Public, 18, false),
            SSTORE,
        ),
    );
    let mut tx = transaction();
    tx.base.kind = TxKind::Call(GAS_TOKEN_REGISTRY);
    let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector);
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    }
    .unwrap();
    assert!(result.is_success());
    let net = TokenPrecision::new(6)
        .unwrap()
        .ceil(U256::from(GAS_LIMIT) * U256::from(PRICE))
        - TokenPrecision::new(6)
            .unwrap()
            .floor(U256::from(GAS_LIMIT - result.gas_used()) * U256::from(PRICE));
    let mut next = transaction();
    next.base.nonce = 1;
    let result = if inspected {
        evm.inspect_one_tx(next)
    } else {
        evm.transact_one(next)
    };
    assert!(matches!(
        result,
        Err(EVMError::Transaction(InvalidTransaction::GasTokenInactive(
            TOKEN
        )))
    ));
    let state = evm.finalize();
    assert!(
        state[&CALLER].is_touched(),
        "discard must preserve the preceding transaction's touch"
    );
    assert_eq!(state[&CALLER].info.nonce, 1);
    evm.ctx().db_mut().commit(state);
    assert_eq!(
        evm.ctx()
            .db_mut()
            .storage(TOKEN, balance_storage_key(CALLER, ROOT))
            .unwrap()
            .value,
        initial - net
    );
    assert_eq!(
        evm.ctx()
            .db_mut()
            .storage(TOKEN, balance_storage_key(BENEFICIARY, ROOT))
            .unwrap()
            .value,
        net
    );
}

#[rstest]
fn positive_maximum_and_zero_effective_price_keep_token_selection_without_writes(
    #[values(BalanceStorageMode::Public, BalanceStorageMode::Shielded)] mode: BalanceStorageMode,
) {
    let (ctx, initial) = context(6, mode, BENEFICIARY);
    let mut tx = transaction();
    tx.base.gas_priority_fee = Some(0); // Positive maximum fee, effective fee zero at basefee zero.
    let mut evm = ctx.build_seismic_evm();
    let result = evm.transact_one(tx).unwrap();
    assert!(result.is_success());
    let state = evm.finalize();
    assert!(!state[&TOKEN].is_touched());
    assert_eq!(
        state[&TOKEN].storage.len(),
        1,
        "zero refund/reward must not read or initialize destinations"
    );
    assert!(
        !state.contains_key(&BENEFICIARY),
        "a zero token reserve must not become native reward processing"
    );
    evm.ctx().db_mut().commit(state);
    assert_eq!(
        evm.ctx()
            .db_mut()
            .storage(TOKEN, balance_storage_key(CALLER, ROOT))
            .unwrap()
            .value,
        initial
    );
    assert_eq!(
        evm.ctx()
            .db_mut()
            .storage(TOKEN, balance_storage_key(BENEFICIARY, ROOT))
            .unwrap(),
        FlaggedStorage::ZERO
    );
}

#[rstest]
fn settlement_mode_mismatch_is_typed_and_discards_body_nonce_and_fees(
    #[values(false, true)] inspected: bool,
) {
    let (mut ctx, initial) = context(6, BalanceStorageMode::Public, BENEFICIARY);
    // Body initializes beneficiary's empty slot as private; Public reward must fail.
    set_code(
        &mut ctx,
        TOKEN,
        store_code(balance_storage_key(BENEFICIARY, ROOT), U256::from(1), 0xb1),
    );
    let mut tx = transaction();
    tx.base.kind = TxKind::Call(TOKEN);
    let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector);
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    };
    assert!(matches!(
        result,
        Err(EVMError::Transaction(
            InvalidTransaction::GasTokenBalanceModeMismatch {
                token: TOKEN,
                account: BENEFICIARY
            }
        ))
    ));
    assert!(evm.ctx().error().is_ok());
    assert!(evm.ctx().chain().token_fee().is_none());
    let state = evm.finalize();
    assert_eq!(state[&CALLER].info.nonce, 0);
    assert!(!state[&CALLER].is_touched());
    assert!(!state[&TOKEN].is_touched());
    evm.ctx().db_mut().commit(state);
    assert_eq!(
        evm.ctx()
            .db_mut()
            .storage(TOKEN, balance_storage_key(CALLER, ROOT))
            .unwrap()
            .value,
        initial
    );
    assert_eq!(
        evm.ctx()
            .db_mut()
            .storage(TOKEN, balance_storage_key(BENEFICIARY, ROOT))
            .unwrap(),
        FlaggedStorage::ZERO
    );
}

#[rstest]
fn explicit_selectors_reject_standard_transactions_even_with_balance_checks_disabled(
    #[values(GasPayment::Native, GasPayment::Token(TOKEN))] selector: GasPayment,
    #[values(false, true)] disabled: bool,
    #[values(false, true)] inspected: bool,
) {
    let (ctx, _) = context(6, BalanceStorageMode::Public, BENEFICIARY);
    let ctx = ctx.modify_cfg_chained(|cfg| cfg.disable_balance_check = disabled);
    let mut tx = transaction();
    tx.base.tx_type = 0;
    tx.gas_payment = selector;
    let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector);
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    };
    assert!(matches!(
        result,
        Err(EVMError::Transaction(
            InvalidTransaction::InvalidGasPaymentSelector
        ))
    ));
    assert!(evm.ctx().chain().token_fee().is_none());
    assert!(evm.ctx().error().is_ok());
    assert!(evm.finalize().values().all(|account| !account.is_touched()));
}

#[rstest]
fn disabled_balance_checks_skip_all_fee_assets_but_commit_caller_nonce(
    #[values(
        GasPayment::Auto,
        GasPayment::Native,
        GasPayment::Token(Address::repeat_byte(0xee))
    )]
    selector: GasPayment,
    #[values(false, true)] inspected: bool,
) {
    let (ctx, _) = context(6, BalanceStorageMode::Shielded, BENEFICIARY);
    let ctx = ctx.modify_cfg_chained(|cfg| cfg.disable_balance_check = true);
    let mut tx = transaction();
    tx.gas_payment = selector;
    let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector);
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    }
    .unwrap();
    assert!(result.is_success());
    assert!(evm.ctx().chain().token_fee().is_none());
    let state = evm.finalize();
    assert!(!state.contains_key(&TOKEN));
    assert!(!state.contains_key(&GAS_TOKEN_REGISTRY));
    assert!(!state.contains_key(&BENEFICIARY));
    assert_eq!(state[&CALLER].info.nonce, 1);
    assert!(state[&CALLER].is_touched());
    assert_eq!(state[&CALLER].info.balance, U256::ZERO);
    evm.ctx().db_mut().commit(state);
    assert_eq!(evm.ctx().db_mut().basic(CALLER).unwrap().unwrap().nonce, 1);
}

#[rstest]
fn failed_decryption_preserves_explicit_selection_when_native_is_affordable(
    #[values(GasPayment::Native, GasPayment::Token(TOKEN))] selector: GasPayment,
    #[values(false, true)] create: bool,
    #[values(false, true)] inspected: bool,
) {
    let (mut ctx, initial_token) = context(6, BalanceStorageMode::Public, BENEFICIARY);
    let initial_native = U256::from(GAS_LIMIT) * U256::from(PRICE) * U256::from(10);
    ctx.db_mut().insert_account_info(
        CALLER,
        AccountInfo {
            balance: initial_native,
            ..Default::default()
        },
    );
    let mut tx = transaction();
    tx.gas_payment = selector;
    tx.decryption_failed = true;
    if create {
        tx.base.kind = TxKind::Create;
    }
    let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector);
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    }
    .unwrap();
    assert!(matches!(
        result,
        revm::context::result::ExecutionResult::Revert { .. }
    ));
    assert_eq!(result.gas_used(), if create { 53_000 } else { 21_000 });
    assert!(evm.ctx().chain().token_fee().is_none());
    let fee_wei = U256::from(result.gas_used()) * U256::from(PRICE);
    let state = evm.finalize();
    assert_eq!(state[&CALLER].info.nonce, 1);
    assert_eq!(
        state[&CALLER].info.balance,
        if selector == GasPayment::Native {
            initial_native - fee_wei
        } else {
            initial_native
        }
    );
    if selector == GasPayment::Native {
        assert!(!state.contains_key(&TOKEN));
        assert!(!state.contains_key(&GAS_TOKEN_REGISTRY));
        assert_eq!(state[&BENEFICIARY].info.balance, fee_wei);
    }
    evm.ctx().db_mut().commit(state);
    let caller_tokens = evm
        .ctx()
        .db_mut()
        .storage(TOKEN, balance_storage_key(CALLER, ROOT))
        .unwrap();
    let beneficiary_tokens = evm
        .ctx()
        .db_mut()
        .storage(TOKEN, balance_storage_key(BENEFICIARY, ROOT))
        .unwrap();
    let fee_tokens = if selector == GasPayment::Native {
        U256::ZERO
    } else {
        TokenPrecision::new(6).unwrap().ceil(fee_wei)
    };
    assert_eq!(caller_tokens.value, initial_token - fee_tokens);
    assert_eq!(beneficiary_tokens.value, fee_tokens);
}

#[test]
fn nonzero_debit_to_zero_retains_private_visibility() {
    let (mut ctx, _) = context(6, BalanceStorageMode::Shielded, CALLER);
    // Consume the entire 1-unit balance upfront with a fee smaller than one unit.
    ctx.db_mut()
        .insert_account_storage(
            TOKEN,
            balance_storage_key(CALLER, ROOT),
            FlaggedStorage::new(U256::from(1), true),
        )
        .unwrap();
    let mut tx = transaction();
    tx.base.gas_price = 1;
    // Observe the debited zero slot via CLOAD, then revert: fees still settle.
    let mut code = vec![PUSH32];
    code.extend_from_slice(&balance_storage_key(CALLER, ROOT).to_be_bytes::<32>());
    code.extend_from_slice(&[0xb0, PUSH1, 0, PUSH1, 0, REVERT]);
    set_code(&mut ctx, TOKEN, code);
    tx.base.kind = TxKind::Call(TOKEN);
    let mut evm = ctx.build_seismic_evm();
    let result = evm.transact_one(tx).unwrap();
    assert!(!result.is_success());
    let state = evm.finalize();
    // Caller==beneficiary returns the full raw unit after the body; it remains private.
    assert_eq!(
        state[&TOKEN].storage[&balance_storage_key(CALLER, ROOT)].present_value,
        FlaggedStorage::new(U256::from(1), true)
    );
}
