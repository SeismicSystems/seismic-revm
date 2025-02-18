use hex::FromHex;
use log::{debug, error};
use revm::{
    inspector_handle_register, inspectors::TracerEip3155, primitives::{
        Address, Bytes, EvmState, ExecutionResult, FixedBytes, HandlerCfg, Output, ResultAndState, SpecId, TxKind, U256
    }, seismic::seismic_handle_register, CacheState, DatabaseCommit, Evm
};

use std::str::FromStr;

use crate::cmd::semantics::utils::verify_emitted_events;

use super::{test_cases::TestCase, utils::{verify_expected_balances, verify_storage_empty}, Errors};

#[derive(Debug, Clone)]
pub(crate) struct EvmConfig {
    pub evm_version: SpecId,
    pub blob_hashes: Vec<FixedBytes<32>>,
    pub max_blob_fee: U256,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub block_gas_limit: U256,
    pub block_prevrandao: FixedBytes<32>,
    pub block_difficulty: FixedBytes<32>,
    pub block_coinbase: Address,
    pub block_basefee: U256,
    pub block_number: U256,
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
        let gas_price = U256::from(8);
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
        let block_basefee = U256::from(7);
        let block_number = U256::from(1);
        let env_contract_address =
            Address::from_hex("0xc06afe3a8444fc0004668591e8306bfb9968e79e").unwrap();
        let caller = Address::from_str("0x1212121212121212121212121212120000000012").unwrap();

        Self {
            evm_version,
            blob_hashes,
            max_blob_fee,
            gas_limit,
            gas_price,
            block_gas_limit,
            block_prevrandao,
            block_difficulty,
            block_coinbase,
            block_basefee,
            block_number,
            env_contract_address,
            caller,
        }
    }
}

pub(crate) struct EvmExecutor {
    cache: CacheState,
    pub config: EvmConfig,
    evm_version: SpecId,
}

impl EvmExecutor {
    pub(crate) fn new(cache: CacheState, config: EvmConfig, evm_version: SpecId) -> Self {
        Self {
            cache,
            config,
            evm_version,
        }
    }

    pub(crate) fn deploy_contract(
        &mut self,
        deploy_data: Bytes,
        test_case: TestCase,
        trace: bool,
    ) -> Result<Address, Errors> {
        let mut state = revm::db::State::builder()
            .with_cached_prestate(self.cache.clone())
            .with_bundle_update()
            .build();

        let (contract_address, deploy_state) = {
            let mut evm = Evm::builder()
                .with_db(&mut state) 
                .modify_tx_env(|tx| {
                    tx.caller = self.config.caller;
                    tx.transact_to = TxKind::Create;
                    tx.data = deploy_data.clone();
                    tx.value = test_case.value;
                })
                .with_handler_cfg(HandlerCfg::new(self.evm_version))
                .append_handler_register(seismic_handle_register)
                .build();

            let deploy_out = if trace {
                let mut evm = evm
                    .modify()
                    .reset_handler_with_external_context(TracerEip3155::new(
                        Box::new(std::io::stdout()),
                    ))
                    .append_handler_register(inspector_handle_register)
                    .append_handler_register(seismic_handle_register)
                    .build();

                evm.transact().map_err(|err| {
                    error!("DEPLOY transaction error: {:?}", err.to_string());
                    Errors::EVMError
                })?
            } else {
                evm.transact().map_err(|err| {
                    error!("DEPLOY transaction error: {:?}", err.to_string());
                    Errors::EVMError
                })?
            };

            let contract_address: Address = match deploy_out.clone().result {
                ExecutionResult::Success { output, logs, .. } => match output {
                    Output::Create(_, Some(addr)) => {
                        verify_emitted_events(&test_case.expected_events, &logs)?;
                        addr
                    }
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

            let deploy_state: EvmState = deploy_out.state;
            
            Ok::<(Address, EvmState), Errors>((contract_address, deploy_state))
        }?; 

        state.commit(deploy_state); // commit deploy_state
        self.cache = state.cache.clone();
        verify_expected_balances(
            &state.cache, 
            &test_case.expected_balances,
            contract_address,
        )?;
        if let Some(expected_empty) = test_case.expected_storage_empty {
            verify_storage_empty(&state.cache, contract_address, expected_empty)?;
        } 

        Ok(contract_address)
    }

    pub(crate) fn copy_contract_to_env(&mut self, contract_address: Address) {
        let (account_info_clone, storage_entries) = {
            let account_info = self.cache.accounts.get(&contract_address).unwrap();
            let account_info_clone = account_info.account_info().clone().unwrap();
            let storage_entries: Vec<_> = account_info
                .account
                .clone()
                .unwrap()
                .storage
                .iter()
                .map(|(slot, value)| (*slot, *value))
                .collect();
            (account_info_clone, storage_entries)
        };
        self.cache
            .insert_account(self.config.env_contract_address, account_info_clone);

        for (slot, value) in storage_entries {
            let _ = self
                .cache
                .accounts
                .get(&self.config.env_contract_address)
                .unwrap()
                .account
                .clone()
                .unwrap()
                .storage
                .insert(slot, value);
        }
    }

    pub(crate) fn run_test_case(
        &mut self,
        test_case: &TestCase,
        trace: bool,
        test_file: &str,
    ) -> Result<(), Errors> {
        debug!("running test_case: {:?}", test_case);

        let mut state = revm::db::State::builder()
            .with_cached_prestate(self.cache.clone())
            .with_bundle_update()
            .build();

        let evm_out = {
            let mut evm = Evm::builder()
                .with_db(&mut state) 
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
                    tx.gas_price = self.config.gas_price;
                })
                .modify_env(|env| {
                    env.block.prevrandao = Some(self.config.block_prevrandao);
                    env.block.difficulty = self.config.block_difficulty.into();
                    env.block.gas_limit = self.config.block_gas_limit;
                    env.block.coinbase = self.config.block_coinbase;
                    env.block.basefee = self.config.block_basefee;
                    env.block.number = self.config.block_number;
                })
                .with_handler_cfg(HandlerCfg::new(self.evm_version))
                .append_handler_register(seismic_handle_register)
                .build();

            let out = if trace {
                let mut evm = evm
                    .modify()
                    .reset_handler_with_external_context(TracerEip3155::new(
                        Box::new(std::io::stdout()),
                    ))
                    .append_handler_register(inspector_handle_register)
                    .append_handler_register(seismic_handle_register)
                    .build();

                evm.transact().map_err(|err| {
                    error!(
                        "EVM transaction error: {:?}, for the file: {:?}",
                        err.to_string(),
                        test_file
                    );
                    Errors::EVMError
                })?
            } else {
                evm.transact().map_err(|err| {
                    error!(
                        "EVM transaction error: {:?}, for the file: {:?}",
                        err.to_string(),
                        test_file
                    );
                    Errors::EVMError
                })?
            };

            Ok::<ResultAndState, Errors>(out)
        }?; 

        match evm_out.clone().result {
            ExecutionResult::Success {
                output,
                reason,
                logs,
                ..
            } => {
                if test_case.expected_outputs.is_success() {
                    match output {
                        Output::Call(out) => {
                            assert_eq!(out, test_case.expected_outputs.output);
                            verify_emitted_events(&test_case.expected_events, &logs)?;
                        }
                        _ => return Err(Errors::EVMError),
                    }
                } else {
                    error!(
                        "An error was expected from the testCase, and yet, the test passed with output: {:?}, reason: {:?}, for file: {:?}",
                        output, reason, test_file
                    );
                    return Err(Errors::EVMError);
                }
            }

            ExecutionResult::Revert { output, .. } => {
                if !test_case.expected_outputs.is_success() {
                    return Ok(());
                } else {
                    error!(
                        "Reverted with output: {:?} for file {:?}",
                        output.to_string(),
                        test_file
                    );
                    assert_eq!(output, test_case.expected_outputs.output);
                }
            }

            ExecutionResult::Halt { reason, .. } => {
                if !test_case.expected_outputs.is_success() {
                    return Ok(());
                } else {
                    error!("Execution halted: {:?} for file {:?}", reason, test_file);
                    return Err(Errors::EVMError);
                }
            }
        };
        

        state.commit(evm_out.state); 
        self.cache = state.cache.clone();
        let finalized_state = self.cache.clone();
        verify_expected_balances(
            &finalized_state,
            &test_case.expected_balances,
            self.config.env_contract_address,
        )?;
        if let Some(expected_empty) = test_case.expected_storage_empty {
            verify_storage_empty(&finalized_state, self.config.env_contract_address, expected_empty)?;
        } 

        Ok(())
    }
}
