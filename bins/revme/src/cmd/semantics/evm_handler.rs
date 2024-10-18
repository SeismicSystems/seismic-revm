use hex::FromHex;
use revm::{
    db::{CacheDB, EmptyDB},
    primitives::{
        Address, Bytes, ExecutionResult, FixedBytes, HandlerCfg, Output, SpecId, TxKind, U256,
    },
    DatabaseCommit, Evm,
};

use std::{str::FromStr, u64};

use super::{semantic_tests::SemanticTests, test_cases::TestCase, Errors};

pub(crate) struct EvmConfig {
    pub blob_hashes: Vec<FixedBytes<32>>,
    pub max_blob_fee: U256,
    pub gas_limit: u64,
    pub block_gas_limit: U256,
    pub block_prevrandao: FixedBytes<32>,
    pub block_difficulty: FixedBytes<32>,
    pub block_coinbase: Address,
    pub env_contract_address: Address,
    pub caller: Address,
}

impl EvmConfig {
    pub(crate) fn new(evm_version: SpecId) -> Self {
        let blob_hashes = if evm_version >= SpecId::CANCUN {
            vec![
                FixedBytes::<32>::from_hex(
                    "0100000000000000000000000000000000000000000000000000000000000001",
                )
                .unwrap(),
                FixedBytes::<32>::from_hex(
                    "0100000000000000000000000000000000000000000000000000000000000002",
                )
                .unwrap(),
            ]
        } else {
            vec![]
        };

        let max_blob_fee = if evm_version >= SpecId::CANCUN {
            U256::from(1)
        } else {
            U256::ZERO
        };

        let block_gas_limit = U256::from(20000000);
        let gas_limit = 20000000 - 10;
        let block_prevrandao = FixedBytes::<32>::from_hex(
            "0xa86c2e601b6c44eb4848f7d23d9df3113fbcac42041c49cbed5000cb4f118777",
        )
        .unwrap();
        let block_difficulty = FixedBytes::<32>::from_hex(
            "0x000000000000000000000000000000000000000000000000000000000bebc200",
        )
        .unwrap();
        let block_coinbase =
            Address::from_hex("0x7878787878787878787878787878787878787878").unwrap();
        let env_contract_address =
            Address::from_hex("0xc06afe3a8444fc0004668591e8306bfb9968e79e").unwrap();
        let caller = Address::from_str("0x1212121212121212121212121212120000000012").unwrap();

        Self {
            blob_hashes,
            max_blob_fee,
            gas_limit,
            block_gas_limit,
            block_prevrandao,
            block_difficulty,
            block_coinbase,
            env_contract_address,
            caller,
        }
    }
}

pub(crate) struct EvmExecutor<'a> {
    db: CacheDB<EmptyDB>,
    config: EvmConfig,
    evm_version: SpecId,
    semantic_tests: &'a SemanticTests,
}

impl<'a> EvmExecutor<'a> {
    pub(crate) fn new(
        db: CacheDB<EmptyDB>,
        config: EvmConfig,
        evm_version: SpecId,
        semantic_tests: &'a SemanticTests,
    ) -> Self {
        Self {
            db,
            config,
            evm_version,
            semantic_tests,
        }
    }

    pub(crate) fn deploy_contract(
        &mut self,
        deploy_data: Bytes,
        value: U256,
    ) -> Result<Address, Errors> {
        let mut evm = Evm::builder()
            .with_db(self.db.clone())
            .modify_tx_env(|tx| {
                tx.caller = self.config.caller;
                tx.transact_to = TxKind::Create;
                tx.data = deploy_data.clone();
                tx.value = value;
            })
            .with_handler_cfg(HandlerCfg::new(self.evm_version))
            .build();

        let deploy_out = evm.transact().map_err(|err| {
            println!("EVM transaction error: {:?}", err);
            Errors::EVMError
        })?;

        let contract_address = match deploy_out.clone().result {
            ExecutionResult::Success { output, .. } => match output {
                Output::Create(_, Some(addr)) => addr,
                Output::Create(_, None) => return Err(Errors::EVMError),
                _ => return Err(Errors::EVMError),
            },
            ExecutionResult::Revert { output, .. } => {
                println!("Execution reverted during deployment: {:?}", output);
                return Err(Errors::EVMError);
            }
            ExecutionResult::Halt { reason, .. } => {
                println!("Execution halted during deployment: {:?}", reason);
                return Err(Errors::EVMError);
            }
        };

        self.db.commit(deploy_out.state);
        Ok(contract_address)
    }

    pub(crate) fn copy_contract_to_env(&mut self, contract_address: Address) {
        let (account_info_clone, storage_entries) = {
            let account_info = self.db.load_account(contract_address).unwrap();
            let account_info_clone = account_info.info.clone();
            let storage_entries: Vec<_> = account_info
                .storage
                .iter()
                .map(|(slot, value)| (slot.clone(), value.clone()))
                .collect();
            (account_info_clone, storage_entries)
        };
        self.db
            .insert_account_info(self.config.env_contract_address, account_info_clone);

        for (slot, value) in storage_entries {
            let _ = self
                .db
                .insert_account_storage(self.config.env_contract_address, slot, value);
        }
    }

    pub(crate) fn run_test_case(&mut self, test_case: &TestCase) -> Result<(), Errors> {
        println!("test_case: {:?}", test_case.function_name);
        let mut evm = Evm::builder()
            .with_db(self.db.clone())
            .modify_tx_env(|tx| {
                tx.caller = self.config.caller;
                tx.transact_to = TxKind::Call(self.config.env_contract_address);
                tx.data = test_case.input_data.clone();
                tx.value = test_case.value;
                if self.evm_version >= SpecId::CANCUN {
                    tx.blob_hashes = self.config.blob_hashes.clone();
                    tx.max_fee_per_blob_gas = Some(self.config.max_blob_fee);
                }
                tx.gas_limit = self.config.gas_limit;
            })
            .modify_env(|env| {
                env.block.prevrandao = Some(self.config.block_prevrandao);
                env.block.difficulty = self.config.block_difficulty.into();
                env.block.gas_limit = self.config.block_gas_limit;
                env.block.coinbase = self.config.block_coinbase;
            })
            .with_handler_cfg(HandlerCfg::new(self.evm_version))
            .build();

        let out = evm.transact().map_err(|err| {
            println!("EVM transaction error: {:?}", err);
            Errors::EVMError
        })?;

        let success_res = match out.clone().result {
            ExecutionResult::Success { output, .. } => match output {
                Output::Call(out) => Bytes::from(out),
                _ => return Err(Errors::EVMError),
            },
            ExecutionResult::Revert { output, .. } => {
                println!("Execution reverted: {:?}", output);
                Bytes::from(U256::ZERO.to_be_bytes::<32>())
            }
            ExecutionResult::Halt { reason, .. } => {
                println!("Execution halted: {:?}", reason);
                return Err(Errors::EVMError);
            }
        };

        assert_eq!(success_res, test_case.expected_outputs);
        self.db.commit(out.state);
        Ok(())
    }
}
