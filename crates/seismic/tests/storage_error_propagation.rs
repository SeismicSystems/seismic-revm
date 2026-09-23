//! Exercise storage-read failures through the real host adapter and EVM handler.

use revm::{
    bytecode::opcode::{PUSH1, SLOAD, STOP},
    context::result::EVMError,
    database_interface::DBErrorMarker,
    primitives::{Address, Bytes, FlaggedStorage, TxKind, B256, U256},
    state::{AccountInfo, Bytecode},
    Context, Database, ExecuteEvm,
};
use seismic_revm::{DefaultSeismicContext, SeismicBuilder};

const CALLER: Address = Address::repeat_byte(0x11);
const CONTRACT: Address = Address::repeat_byte(0x22);
const CLOAD: u8 = 0xb0;

#[derive(Debug)]
struct StorageReadError;

impl core::fmt::Display for StorageReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("injected storage read failure")
    }
}

impl core::error::Error for StorageReadError {}
impl DBErrorMarker for StorageReadError {}

/// Account/code reads succeed so execution reaches the failing storage read.
#[derive(Debug)]
struct FailingStorageDb {
    code: Bytecode,
}

impl Database for FailingStorageDb {
    type Error = StorageReadError;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        Ok(if address == CONTRACT {
            Some(AccountInfo {
                code_hash: self.code.hash_slow(),
                code: Some(self.code.clone()),
                nonce: 1,
                ..Default::default()
            })
        } else if address == CALLER {
            // Avoid the ERC20 gas-payment path, which also accesses storage.
            Some(AccountInfo {
                balance: U256::from(1_000_000_000_000_000_000u64),
                ..Default::default()
            })
        } else {
            None
        })
    }

    fn code_by_hash(&mut self, _: B256) -> Result<Bytecode, Self::Error> {
        Ok(self.code.clone())
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<FlaggedStorage, Self::Error> {
        assert_eq!(address, CONTRACT);
        assert_eq!(index, U256::ZERO);
        Err(StorageReadError)
    }

    fn block_hash(&mut self, _: u64) -> Result<B256, Self::Error> {
        Ok(B256::ZERO)
    }
}

fn assert_storage_error_propagates(opcode: u8) {
    // PUSH1 0; <storage load>; STOP. The slot must be fetched from the database.
    let code = Bytecode::new_raw(Bytes::from(vec![PUSH1, 0, opcode, STOP]));
    let ctx = Context::seismic_with_rng_key([0; 64])
        .modify_tx_chained(|tx| {
            tx.base.caller = CALLER;
            tx.base.kind = TxKind::Call(CONTRACT);
            tx.base.gas_limit = 100_000;
            tx.base.gas_price = 0;
        })
        .with_db(FailingStorageDb { code });
    let mut evm = ctx.build_seismic_evm();

    // FatalExternalError must be accompanied by a recorded Context.error so
    // the handler returns the original database error instead of panicking.
    let result = evm.replay();
    assert!(
        matches!(result, Err(EVMError::Database(StorageReadError))),
        "expected the storage database error, got {result:?}"
    );
}

#[test]
fn cload_propagates_storage_database_error() {
    assert_storage_error_propagates(CLOAD);
}

/// Control: the public load must propagate the same failure normally.
#[test]
fn sload_propagates_storage_database_error() {
    assert_storage_error_propagates(SLOAD);
}
