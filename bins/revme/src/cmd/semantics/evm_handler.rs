use context::result::{ExecutionResult, Output};
use handler::EvmTr;
use log::{debug, error};
use primitives::{hardfork::SpecId, hex::FromHex};
use revm::{
    database::{CacheDB, EmptyDB}, inspector::inspectors::TracerEip3155, primitives::{
        hex, Address, Bytes, FixedBytes, Log, TxKind, U256
    }, Context, DatabaseCommit, ExecuteEvm, MainBuilder 
};
use seismic_revm::{DefaultSeismic, SeismicBuilder};
use std::str::FromStr;

use crate::cmd::semantics::{test_cases::TestStep, utils::verify_emitted_events};

use super::{
    test_cases::{ExpectedOutputs, TestCase},
    utils::{verify_expected_balances, verify_storage_empty},
    Errors,
};

#[derive(Debug, Clone)]
pub(crate) struct EvmConfig {
    pub blob_hashes: Vec<FixedBytes<32>>,
    pub max_blob_fee: u128,
    pub gas_limit: u64,
    pub timestamp: u64,
    pub gas_price: u128,
    pub block_gas_limit: u64,
    pub block_prevrandao: FixedBytes<32>,
    pub block_difficulty: FixedBytes<32>,
    pub block_basefee: u64,
    pub block_number: u64,
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
            1 as u128
        } else {
            0 as u128
        };

        let block_gas_limit = 20000000_u64;
        let gas_limit = 20000000 - 10;
        let gas_price = 3000000000_u128;
        let block_prevrandao = FixedBytes::<32>::from_hex(
            "0xa86c2e601b6c44eb4848f7d23d9df3113fbcac42041c49cbed5000cb4f118777",
        )
        .unwrap();
        let block_difficulty = FixedBytes::<32>::from_hex(
            "0x000000000000000000000000000000000000000000000000000000000bebc200",
        )
        .unwrap();
        let block_basefee = 7_u64;
        let block_number = 1_u64;
        let timestamp = 15_u64;
        let env_contract_address =
            Address::from_hex("0xc06afe3a8444fc0004668591e8306bfb9968e79e").unwrap();
        let caller = Address::from_str("0x1212121212121212121212121212120000000012").unwrap();

        Self {
            blob_hashes,
            max_blob_fee,
            gas_limit,
            gas_price,
            timestamp,
            block_gas_limit,
            block_prevrandao,
            block_difficulty,
            block_basefee,
            block_number,
            env_contract_address,
            caller,
        }
    }
}

pub(crate) struct EvmExecutor {
    db: CacheDB<EmptyDB>,
    pub config: EvmConfig,
    evm_version: SpecId,
    libraries: Vec<Address>,
}

impl EvmExecutor {
    pub(crate) fn new(db: CacheDB<EmptyDB>, config: EvmConfig, evm_version: SpecId) -> Self {
        Self {
            db,
            config,
            evm_version,
            libraries: Vec::new(),
        }
    }

    pub(crate) fn deploy_contract(
        &mut self,
        deploy_data: Bytes,
        trace: bool,
        value: U256,
    ) -> Result<(Address, Vec<Log>), Errors> {

        let deploy_out = if trace {
            let evm = Context::seismic()
                .with_db(self.db.clone())
                .modify_tx_chained(|tx| {
                    tx.base.caller = self.config.caller;
                    tx.base.kind = TxKind::Create;
                    tx.base.data = deploy_data.clone();
                    tx.base.value = value
                })
                .modify_cfg_chained(|cfg| cfg.spec = self.evm_version)
                .build_mainnet_with_inspector(TracerEip3155::new(
                    Box::new(std::io::stdout())));

            evm.replay().map_err(|err| {
                error!("DEPLOY transaction error: {:?}", err.to_string());
                Errors::EVMError
            })?
        } else {
            let evm = Context::seismic()
                .with_db(self.db.clone())
                .modify_tx_chained(|tx| {
                    tx.base.caller = self.config.caller;
                    tx.base.kind = TxKind::Create;
                    tx.base.data = deploy_data.clone();
                    tx.base.value = value
                })
                .modify_cfg_chained(|cfg| cfg.spec = self.evm_version)
                .build_seismic();

            evm.replay().map_err(|err| {
                error!("DEPLOY transaction error: {:?}", err.to_string());
                Errors::EVMError
            })?
        };

        let (contract_address, logs) = match deploy_out.clone().result {
            ExecutionResult::Success { output, logs, .. } => match output {
                Output::Create(_, Some(addr)) => (addr, logs),
                Output::Create(_, None) => return Err(Errors::EVMError),
                _ => return Err(Errors::EVMError),
            },
            ExecutionResult::Revert { output, .. } => {
                error!("EVM deploy transaction error: {:?}", output.to_string());
                return Err(Errors::EVMError);
            }
            ExecutionResult::Halt { reason, .. } => {
                error!("Execution halted during deployment: {:?}", reason);
                return Err(Errors::EVMError);
            }
        };

        self.db.commit(deploy_out.state);
        Ok((contract_address, logs))
    }

    pub(crate) fn execute_function_call(
        &mut self,
        function_name: &str,
        input_data: &Bytes,
        expected_outputs: &ExpectedOutputs,
        trace: bool,
        test_file: &str,
        value: U256,
    ) -> Result<Vec<Log>, Errors> {
        let out = if trace {
            let evm = Context::seismic()
                .with_db(self.db.clone())
                .modify_tx_chained(|tx| {
                    tx.base.caller = self.config.caller;
                    tx.base.kind = TxKind::Call(self.config.env_contract_address);
                    tx.base.data = input_data.clone();
                    tx.base.value = value;
                    if self.evm_version >= SpecId::CANCUN {
                        tx.base.blob_hashes = self.config.blob_hashes.clone();
                        tx.base.max_fee_per_blob_gas = self.config.max_blob_fee;
                }
                    tx.base.gas_limit = self.config.gas_limit;
                    tx.base.gas_price = self.config.gas_price;
                })
                .modify_block_chained(|block| {
                    block.prevrandao = Some(self.config.block_prevrandao);
                    block.difficulty = self.config.block_difficulty.into();
                    block.gas_limit = self.config.block_gas_limit;
                    block.basefee = self.config.block_basefee;
                    block.number = self.config.block_number;
                    block.timestamp = self.config.timestamp;
                })
                .modify_cfg_chained(|cfg| cfg.spec = self.evm_version)
                .build_mainnet_with_inspector(TracerEip3155::new(
                    Box::new(std::io::stdout())));

            evm.replay().map_err(|err| {
                error!(
                    "EVM transaction error: {:?}, for the file: {:?}",
                    err.to_string(),
                    test_file
                );
                Errors::EVMError
            })?
        } else {
            let evm = Context::seismic()
                .with_db(self.db.clone())
                .modify_tx_chained(|tx| {
                    tx.base.caller = self.config.caller;
                    tx.base.kind = TxKind::Call(self.config.env_contract_address);
                    tx.base.data = input_data.clone();
                    tx.base.value = value;
                    if self.evm_version >= SpecId::CANCUN {
                        tx.base.blob_hashes = self.config.blob_hashes.clone();
                        tx.base.max_fee_per_blob_gas = self.config.max_blob_fee;
                }
                    tx.base.gas_limit = self.config.gas_limit;
                    tx.base.gas_price = self.config.gas_price;
                })
                .modify_block_chained(|block| {
                    block.prevrandao = Some(self.config.block_prevrandao);
                    block.difficulty = self.config.block_difficulty.into();
                    block.gas_limit = self.config.block_gas_limit;
                    block.basefee = self.config.block_basefee;
                    block.number = self.config.block_number;
                    block.timestamp = self.config.timestamp;
                })
                .modify_cfg_chained(|cfg| cfg.spec = self.evm_version)
                .build_seismic();
            evm.replay().map_err(|err| {
                error!(
                    "EVM transaction error: {:?}, for the file: {:?}",
                    err.to_string(),
                    test_file
                );
                Errors::EVMError
            })?
        };

        let logs = match out.clone().result {
            ExecutionResult::Success { output, logs, .. } => {
                if expected_outputs.is_success() {
                    match output {
                        Output::Call(out) => {
                            assert_eq!(out, expected_outputs.output);
                        }
                        _ => return Err(Errors::EVMError),
                    }
                    logs
                } else {
                    error!("an Error was expected from the testCase, and yet, the test passed with output: {:?}, for file: {:?}", output, test_file);
                    return Err(Errors::EVMError);
                }
            }

            ExecutionResult::Revert { output, .. } => {
                if !expected_outputs.is_success() {
                    return Ok(vec![]);
                } else {
                    error!(
                        "Reverted with output: {:?} for file {:?}",
                        output.to_string(),
                        test_file
                    );
                    assert_eq!(output, expected_outputs.output);
                    vec![]
                }
            }

            ExecutionResult::Halt { reason, .. } => {
                if !expected_outputs.is_success() {
                    return Ok(vec![]);
                } else {
                    error!("Execution halted: {:?} for file {:?}", reason, test_file);
                    return Err(Errors::EVMError);
                }
            }
        };

        self.db.commit(out.state);
        Ok(logs)
    }

    pub(crate) fn copy_contract_to_env(&mut self, contract_address: Address) {
        let (account_info_clone, storage_entries) = {
            let account_info = self.db.load_account(contract_address).unwrap();
            let account_info_clone = account_info.info.clone();
            let storage_entries: Vec<_> = account_info
                .storage
                .iter()
                .map(|(slot, value)| (*slot, *value))
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

    pub(crate) fn run_test_case(
        &mut self,
        test_case: &TestCase,
        trace: bool,
        test_file: &str,
    ) -> Result<(), Errors> {
        debug!("running test_case: {:?}", test_case);
        for step in &test_case.steps {
            match step {
                TestStep::Deploy {
                    contract,
                    value,
                    expected_events,
                } => {
                    if contract.is_library {
                        let (address, _) = self.deploy_contract(
                            contract.clone().get_deployable_code(None),
                            trace,
                            *value,
                        )?;
                        self.libraries.push(address);
                    } else {
                        let (contract_address, logs) = if !self.libraries.is_empty() {
                            self.deploy_contract(
                                contract
                                    .clone()
                                    .get_deployable_code(Some(*self.libraries.first().unwrap())),
                                trace,
                                *value,
                            )?
                        } else {
                            self.deploy_contract(
                                contract.clone().get_deployable_code(None),
                                trace,
                                *value,
                            )?
                        };
                        verify_emitted_events(expected_events, &logs)?;
                        self.copy_contract_to_env(contract_address);
                    }
                }
                TestStep::CallFunction {
                    function_name,
                    input_data,
                    expected_outputs,
                    value,
                    expected_events,
                } => {
                    let logs = self.execute_function_call(
                        function_name,
                        input_data,
                        expected_outputs,
                        trace,
                        test_file,
                        *value,
                    )?;
                    verify_emitted_events(expected_events, &logs)?;
                }
                TestStep::CheckStorageEmpty { expected_empty } => {
                    verify_storage_empty(
                        self.db.clone(),
                        self.config.env_contract_address,
                        *expected_empty,
                    )?;
                }
                TestStep::CheckBalance { expected_balances } => {
                    verify_expected_balances(
                        self.db.clone(),
                        expected_balances,
                        self.config.env_contract_address,
                    )?;
                }
            }
        }
        Ok(())
    }
}
