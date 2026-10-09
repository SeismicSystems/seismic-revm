//! Exact sub-second timestamps must survive normal, inspected and system execution.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use revm::{
    context::{result::ExecutionResult, BlockEnv, TxEnv},
    database::InMemoryDB,
    handler::system_call::SystemCallEvm,
    inspector::{InspectEvm, InspectSystemCallEvm, NoOpInspector},
    primitives::{address, bytes, Address, Bytes, TxKind, B256, U256},
    state::{AccountInfo, Bytecode},
    Context, ExecuteCommitEvm, ExecuteEvm,
};
use rstest::rstest;
use seismic_revm::{DefaultSeismicContext, SeismicBlockEnv, SeismicBuilder};

const CALLER: Address = Address::repeat_byte(0x11);
const CONTRACT: Address = Address::repeat_byte(0x22);
const BEACON_ROOTS_ADDRESS: Address = address!("000f3df6d732807ef1319fb7b8bb8522d0beac02");

fn block(seconds: u64, part: u64) -> SeismicBlockEnv {
    SeismicBlockEnv {
        inner: BlockEnv {
            timestamp: U256::from(seconds),
            ..Default::default()
        },
        timestamp_millis_part: part,
    }
}

fn contract_db(address: Address, code: Bytes) -> InMemoryDB {
    let code = Bytecode::new_legacy(code);
    let mut db = InMemoryDB::default();
    db.insert_account_info(
        address,
        AccountInfo {
            code_hash: code.hash_slow(),
            code: Some(code),
            nonce: 1,
            ..Default::default()
        },
    );
    db.insert_account_info(
        CALLER,
        AccountInfo {
            balance: U256::from(1_000_000_000),
            ..Default::default()
        },
    );
    db
}

#[rstest]
fn timestamp_opcodes_preserve_seconds_and_exact_millis(
    #[values(false, true)] inspected: bool,
    #[values(false, true)] system_call: bool,
) {
    // TIMESTAMP; PUSH0; MSTORE; TIMESTAMPMS; PUSH1 32; MSTORE;
    // PUSH1 64; PUSH0; RETURN.
    let db = contract_db(CONTRACT, bytes!("425f524b60205260405ff3"));
    let ctx = Context::seismic_with_rng_key([0; 64]).with_db(db);
    let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector);

    for (seconds, part) in [
        (0, 0),
        (0, 999),
        (1_700_000_000u64, 0),
        (1_700_000_000, 123),
        (1_700_000_000, 999),
        (1_700_000_001, 0),
    ] {
        evm.set_block(block(seconds, part));
        let tx = TxEnv {
            caller: CALLER,
            kind: TxKind::Call(CONTRACT),
            gas_limit: 100_000,
            ..Default::default()
        }
        .into();
        let result = match (inspected, system_call) {
            (false, false) => evm.transact_one(tx),
            (true, false) => evm.inspect_one_tx(tx),
            (false, true) => evm.system_call_one(CONTRACT, Bytes::new()),
            (true, true) => evm.inspect_one_system_call(CONTRACT, Bytes::new()),
        }
        .unwrap();
        assert!(result.is_success(), "{result:?}");
        let output = result.output().unwrap();
        assert_eq!(output.len(), 64);
        assert_eq!(U256::from_be_slice(&output[..32]), U256::from(seconds));
        assert_eq!(
            U256::from_be_slice(&output[32..]),
            U256::from(seconds) * U256::from(1000) + U256::from(part)
        );
        // Discard the state between calls so the caller nonce stays unchanged.
        let _ = evm.finalize();
    }
}

#[rstest]
fn same_second_blocks_keep_distinct_beacon_root_entries(#[values(false, true)] inspected: bool) {
    // The actual Seismic genesis EIP-4788 bytecode. Its write path uses
    // TIMESTAMPMS (0x4B), unlike Ethereum's seconds-indexed contract.
    let code = bytes!("3373fffffffffffffffffffffffffffffffffffffffe14604d57602036146024575f5ffd5b5f35801560495762001fff810690815414603c575f5ffd5b62001fff01545f5260205ff35b5f5ffd5b62001fff4b064b81555f359062001fff015500");
    let db = contract_db(BEACON_ROOTS_ADDRESS, code);
    let ctx = Context::seismic_with_rng_key([0; 64]).with_db(db);
    let mut evm = ctx.build_seismic_evm_with_inspector(NoOpInspector);
    let seconds = 1_700_000_000;
    let entries = [
        (123, B256::repeat_byte(0x11)),
        (456, B256::repeat_byte(0x22)),
    ];

    for (part, root) in entries {
        evm.set_block(block(seconds, part));
        let data = Bytes::copy_from_slice(root.as_slice());
        let result = if inspected {
            evm.inspect_one_system_call(BEACON_ROOTS_ADDRESS, data)
        } else {
            evm.system_call_one(BEACON_ROOTS_ADDRESS, data)
        }
        .unwrap();
        assert!(result.is_success(), "{result:?}");
        let state = evm.finalize();
        evm.commit(state);
    }

    // Query both entries after the second write. A whole-second TIMESTAMPMS
    // implementation would overwrite the first root and make these lookups fail.
    for (part, root) in entries {
        let millis = seconds * 1000 + part;
        let tx = TxEnv {
            caller: CALLER,
            kind: TxKind::Call(BEACON_ROOTS_ADDRESS),
            gas_limit: 100_000,
            data: Bytes::copy_from_slice(&U256::from(millis).to_be_bytes::<32>()),
            ..Default::default()
        }
        .into();
        let result = if inspected {
            evm.inspect_one_tx(tx)
        } else {
            evm.transact_one(tx)
        }
        .unwrap();
        assert!(result.is_success(), "root lookup for {millis}: {result:?}");
        assert_eq!(result.output().unwrap().as_ref(), root.as_slice());
        let _ = evm.finalize();
    }

    // A seconds timestamp is not an alias for its millisecond entry.
    let tx = TxEnv {
        caller: CALLER,
        kind: TxKind::Call(BEACON_ROOTS_ADDRESS),
        gas_limit: 100_000,
        data: Bytes::copy_from_slice(&U256::from(seconds).to_be_bytes::<32>()),
        ..Default::default()
    }
    .into();
    let result = if inspected {
        evm.inspect_one_tx(tx)
    } else {
        evm.transact_one(tx)
    }
    .unwrap();
    assert!(matches!(result, ExecutionResult::Revert { .. }));
}
