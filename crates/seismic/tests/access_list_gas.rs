//! Seismic accepts access lists without charging gas or warming execution state.

use core::convert::Infallible;
use revm::{
    bytecode::opcode::{BALANCE, POP, PUSH1, PUSH20, SLOAD, STOP},
    context::result::{EVMError, ExecutionResult, InvalidTransaction},
    context_interface::transaction::{AccessList, AccessListItem},
    database::InMemoryDB,
    primitives::{Address, Bytes, FlaggedStorage, TxKind, B256, U256},
    state::{AccountInfo, Bytecode},
    Context, ExecuteEvm,
};
use seismic_revm::{DefaultSeismicContext, SeismicBuilder};

const CALLER: Address = Address::repeat_byte(0x11);
const CONTRACT: Address = Address::repeat_byte(0x22);
const PROBE: Address = Address::repeat_byte(0x33);

fn execute(
    code: &[u8],
    access_list: AccessList,
    gas_limit: u64,
    private_slot: bool,
) -> Result<ExecutionResult, EVMError<Infallible>> {
    let mut db = InMemoryDB::default();
    db.insert_account_info(
        CALLER,
        AccountInfo {
            balance: U256::from(1_000_000_000_000_000_000u64),
            ..Default::default()
        },
    );
    let code = Bytecode::new_raw(Bytes::copy_from_slice(code));
    db.insert_account_info(
        CONTRACT,
        AccountInfo {
            code_hash: code.hash_slow(),
            code: Some(code),
            nonce: 1,
            ..Default::default()
        },
    );
    db.insert_account_storage(
        CONTRACT,
        U256::ZERO,
        FlaggedStorage::new(U256::from(7), private_slot),
    )?;
    let mut evm = Context::seismic_with_rng_key([0; 64])
        .with_db(db)
        .modify_tx_chained(|tx| {
            tx.base.tx_type = 1; // EIP-2930
            tx.base.caller = CALLER;
            tx.base.kind = TxKind::Call(CONTRACT);
            tx.base.gas_limit = gas_limit;
            tx.base.gas_price = 0;
            tx.base.access_list = access_list;
        })
        .build_seismic_evm();
    evm.replay().map(|output| output.result)
}

fn storage_access_list() -> AccessList {
    AccessList(vec![AccessListItem {
        address: CONTRACT,
        storage_keys: vec![B256::ZERO],
    }])
}

#[test]
fn access_list_entries_do_not_change_receipt_gas() -> Result<(), EVMError<Infallible>> {
    for (access_list, expected_gas) in [
        (AccessList::default(), 21_000),
        (
            AccessList(vec![AccessListItem {
                address: CONTRACT,
                storage_keys: vec![],
            }]),
            21_000,
        ),
        (storage_access_list(), 21_000),
        // Duplicate addresses and keys are ignored too.
        (
            AccessList(vec![
                AccessListItem {
                    address: CONTRACT,
                    storage_keys: vec![B256::ZERO, B256::ZERO],
                },
                AccessListItem {
                    address: CONTRACT,
                    storage_keys: vec![B256::ZERO],
                },
            ]),
            21_000,
        ),
    ] {
        let result = execute(&[], access_list, expected_gas, false)?;
        assert!(result.is_success(), "{result:?}");
        assert_eq!(result.gas_used(), expected_gas);
    }
    Ok(())
}

#[test]
fn ignoring_access_lists_does_not_skip_base_intrinsic_gas() {
    for gas_limit in [0, 20_999] {
        let result = execute(&[], storage_access_list(), gas_limit, false);
        assert!(matches!(
            result,
            Err(EVMError::Transaction(
                InvalidTransaction::CallGasCostMoreThanGasLimit {
                    initial_gas: 21_000,
                    ..
                }
            ))
        ));
    }
}

#[test]
fn access_lists_do_not_warm_accounts_or_storage() -> Result<(), EVMError<Infallible>> {
    let mut code = vec![PUSH1, 0, SLOAD, POP, PUSH20];
    code.extend_from_slice(PROBE.as_slice());
    code.extend_from_slice(&[BALANCE, POP, STOP]);
    let mut access_list = storage_access_list();
    access_list.0.push(AccessListItem {
        address: PROBE,
        storage_keys: vec![],
    });
    let without = execute(&code, AccessList::default(), 100_000, false)?;
    let with = execute(&code, access_list, 100_000, false)?;
    assert!(without.is_success() && with.is_success());
    // Both SLOAD and BALANCE must still pay their cold costs.
    assert_eq!(without.gas_used(), 21_000 + 3 + 2_100 + 2 + 3 + 2_600 + 2);
    assert_eq!(with.gas_used(), without.gas_used());
    Ok(())
}

#[test]
fn confidential_slots_in_access_lists_are_ignored() -> Result<(), EVMError<Infallible>> {
    let code = [PUSH1, 0, 0xb0, STOP]; // CLOAD a private slot.
    let without = execute(&code, AccessList::default(), 100_000, true)?;
    let with = execute(&code, storage_access_list(), 100_000, true)?;
    assert!(without.is_success() && with.is_success());
    assert_eq!(without.gas_used(), 21_000 + 3 + 2_100);
    assert_eq!(with.gas_used(), without.gas_used());
    Ok(())
}
