//! Failed calldata decryption must skip contract execution, including with inspection.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use revm::{
    bytecode::opcode::{LOG0, MSTORE8, PUSH1, RETURN, SSTORE},
    context::{result::ExecutionResult, CfgEnv},
    database::InMemoryDB,
    inspector::{InspectEvm, NoOpInspector},
    primitives::{Address, Bytes, TxKind, U256},
    state::{AccountInfo, Bytecode},
    Context, ExecuteEvm,
};
use rstest::rstest;
use seismic_revm::{DefaultSeismicContext, SeismicBuilder, SeismicSpecId};

const CALLER: Address = Address::repeat_byte(0x11);
const CONTRACT: Address = Address::repeat_byte(0x22);
const BENEFICIARY: Address = Address::repeat_byte(0x33);
const CALLER_NONCE: u64 = 7;
const CALLER_BALANCE: u64 = 1_000_000;
const BENEFICIARY_BALANCE: u64 = 1_000;
const GAS_PRICE: u128 = 3;

#[rstest]
fn decryption_guard_is_consistent_with_and_without_inspection(
    #[values(false, true)] inspected: bool,
    #[values(false, true)] create: bool,
    #[values(false, true)] decryption_failed: bool,
    #[values(0, 7)] value: u64,
) {
    // SSTORE(0, 1); LOG0(empty); return a one-byte STOP runtime. This writes
    // storage and emits a log both as target bytecode and as CREATE init code.
    let code_bytes = Bytes::from(vec![
        PUSH1, 1, PUSH1, 0, SSTORE, PUSH1, 0, PUSH1, 0, LOG0, PUSH1, 0, PUSH1, 0, MSTORE8, PUSH1,
        1, PUSH1, 0, RETURN,
    ]);
    let code = Bytecode::new_legacy(code_bytes.clone());
    let target = if create {
        CALLER.create(CALLER_NONCE)
    } else {
        CONTRACT
    };
    let mut db = InMemoryDB::default();
    db.insert_account_info(
        CALLER,
        AccountInfo {
            balance: U256::from(CALLER_BALANCE),
            nonce: CALLER_NONCE,
            ..Default::default()
        },
    );
    db.insert_account_info(
        BENEFICIARY,
        AccountInfo {
            balance: U256::from(BENEFICIARY_BALANCE),
            ..Default::default()
        },
    );
    db.insert_account_info(
        CONTRACT,
        AccountInfo {
            code_hash: code.hash_slow(),
            code: Some(code),
            nonce: 1,
            ..Default::default()
        },
    );

    let ctx = Context::seismic_with_rng_key([0; 64])
        .with_cfg(CfgEnv::new_with_spec(SeismicSpecId::MERCURY))
        .modify_block_chained(|block| {
            block.basefee = 1;
            block.beneficiary = BENEFICIARY;
        })
        .modify_tx_chained(|tx| {
            tx.base.tx_type = 74;
            tx.base.caller = CALLER;
            tx.base.nonce = CALLER_NONCE;
            tx.base.kind = if create {
                TxKind::Create
            } else {
                TxKind::Call(CONTRACT)
            };
            tx.base.gas_limit = 200_000;
            tx.base.gas_price = GAS_PRICE;
            tx.base.gas_priority_fee = None;
            tx.base.value = U256::from(value);
            tx.base.data = if create {
                code_bytes
            } else {
                Bytes::from(vec![0xff; 32])
            };
            tx.decryption_failed = decryption_failed;
        })
        .with_db(db);
    let tx = ctx.tx.clone();
    // Use the same EVM type and a no-op inspector on both paths so the only
    // difference is the public transaction execution entry point.
    let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector);
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    }
    .unwrap();
    let state = evm.finalize();
    let executed = !decryption_failed;
    let target_account = state.get(&target);
    let slot = target_account
        .and_then(|account| account.storage.get(&U256::ZERO))
        .map(|slot| slot.present_value.value)
        .unwrap_or_default();
    let target_balance = target_account
        .map(|account| account.info.balance)
        .unwrap_or_default();

    assert_eq!(
        slot,
        U256::from(u8::from(executed)),
        "failed decryption must not execute SSTORE"
    );
    assert_eq!(
        result.logs().len(),
        usize::from(executed),
        "failed decryption must not emit contract logs"
    );
    assert_eq!(
        target_balance,
        if executed {
            U256::from(value)
        } else {
            U256::ZERO
        },
        "failed decryption must not transfer execution value"
    );
    if create {
        assert_eq!(
            target_account.is_some_and(|account| account.is_created()),
            executed,
            "failed decryption must not create a contract"
        );
        if executed {
            assert_eq!(
                target_account
                    .unwrap()
                    .info
                    .code
                    .as_ref()
                    .unwrap()
                    .bytecode(),
                &Bytes::from_static(&[0]),
                "normal CREATE must install the returned runtime code"
            );
        }
        // CREATE nonce consumption is a separate known defect, deliberately
        // excluded from this inspector-guard regression.
    } else {
        assert_eq!(state[&CALLER].info.nonce, CALLER_NONCE + 1);
    }

    if decryption_failed {
        assert!(
            matches!(&result, ExecutionResult::Revert { output, .. } if output.is_empty()),
            "failed decryption must return an empty revert, got {result:?}"
        );
        // Mercury enables Prague. CALL's 32 nonzero bytes require the 22,280
        // calldata floor. CREATE's 20-byte init code contains 14 nonzero and
        // six zero bytes: 53,000 + 14*16 + 6*4 + 2 (one init-code word).
        let expected_gas = if create { 53_250 } else { 22_280 };
        assert_eq!(result.gas_used(), expected_gas);
    } else {
        assert!(result.is_success(), "normal execution failed: {result:?}");
    }

    let fee = U256::from(GAS_PRICE * u128::from(result.gas_used()));
    let transferred_value = if executed {
        U256::from(value)
    } else {
        U256::ZERO
    };
    assert_eq!(
        state[&CALLER].info.balance,
        U256::from(CALLER_BALANCE) - fee - transferred_value,
        "sender must pay only the execution result's fee and actual value transfer"
    );
    assert_eq!(
        state[&BENEFICIARY].info.balance,
        U256::from(BENEFICIARY_BALANCE) + fee,
        "beneficiary must receive the full gas price, including basefee"
    );
}
