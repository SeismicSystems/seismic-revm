use hex::FromHex;
use k256::elliptic_curve::rand_core::block;
use revm::{
    db::{BenchmarkDB, CacheDB, EmptyDB}, inspector_handle_register, inspectors::TracerEip3155, primitives::{AccountInfo, Address, Bytecode, BytecodeDecodeError, ExecutionResult, Output, TxKind, U256, Bytes, FixedBytes}, DatabaseCommit, Evm
};

use std::{path::PathBuf, str::FromStr};
use structopt::StructOpt;

extern crate alloc;

mod errors;
pub use errors::Errors;
mod semantic_tests;
use semantic_tests::SemanticTests;
mod test_cases;
mod compiler_evm_versions;
mod evm_handler;
mod parser;
mod utils;
use utils::find_test_files;

/// EVM runner command that allows running Solidity semantic tests.
/// If a path is provided, it will process that file or recursively process all `.sol` files in that directory.
/// If no path is provided, it defaults to the Solidity semantic tests directory.
#[derive(StructOpt, Debug)]
pub struct Cmd {
    /// Path to a Solidity file or directory containing Solidity files. If no file is provided, it will default to the Solidity semantic tests directory.
    #[structopt(long)]
    path: Option<PathBuf>,
    /// Print the trace.
    #[structopt(long)]
    trace: bool,
}

impl Cmd {
    pub fn run(&self) -> Result<(), Errors> {
        let test_files = if let Some(ref path) = self.path {
            if path.is_file() {
                vec![path.clone()]
            } else if path.is_dir() {
                find_test_files(path)?
            } else {
                return Err(Errors::PathNotExists);
            }
        } else {
            let current_dir = std::env::current_dir()?;
            let parent_dir = current_dir.parent().ok_or(Errors::PathNotExists)?.parent().ok_or(Errors::PathNotExists)?.parent().ok_or(Errors::PathNotExists)?;
            let semantic_tests_path = parent_dir.join("seismic-solidity-new/test/libsolidity/semanticTests/");
            find_test_files(&semantic_tests_path)?
        };

for test_file in test_files {
    println!("test_file: {:?}", test_file);
    let test_file_path = test_file.to_str().ok_or(Errors::InvalidTestFormat)?;
    let blob_hash_1 = FixedBytes::<32>::from_hex("0100000000000000000000000000000000000000000000000000000000000001").unwrap();
    let blob_hash_2 = FixedBytes::<32>::from_hex("0100000000000000000000000000000000000000000000000000000000000002").unwrap();
    let blob_hashes = vec![blob_hash_1, blob_hash_2];
    let max_blob_fee = U256::from(1);
    let block_prevrando = FixedBytes::<32>::from_hex("0xa86c2e601b6c44eb4848f7d23d9df3113fbcac42041c49cbed5000cb4f118777").unwrap();
    let block_difficulty = FixedBytes::<32>::from_hex("0x000000000000000000000000000000000000000000000000000000000bebc200").unwrap();
    let env_contract_address = Address::from_hex("0xc06afe3a8444fc0004668591e8306bfb9968e79e").unwrap();
    match SemanticTests::new(test_file_path) {
        Ok(semantic_tests) => {
            // Create a mutable database instance to share between transactions
            let mut db = CacheDB::new(EmptyDB::default());

            // Find the constructor test case, if any
            let constructor_test_case = semantic_tests
                .test_cases
                .iter()
                .find(|test_case| test_case.is_constructor)
                .cloned();

            let deployer = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
            
            let account_info = AccountInfo {
                balance: U256::MAX,
                nonce: Default::default(),
                code: None,
                code_hash: Default::default(),
            };

            // Insert or update the account in the database
            db.insert_account_info(deployer, account_info.clone());
            
            // Prepare deploy data
            let deploy_data = if let Some(ref test_case) = constructor_test_case {
                // We have a constructor test case
                // Append the constructor arguments to the deploy binary
                test_case.deploy_binary.clone()
            } else {
                // No constructor test case
                // Use deploy binary without constructor arguments
                // Assume we can get it from the first test case or elsewhere
                if let Some(first_test_case) = semantic_tests.test_cases.first() {
                    first_test_case.deploy_binary.clone()
                } else {
                    continue;
                }
            };

            // Build EVM instance for deployment with the database
            let mut evm = Evm::builder()
                .with_db(db.clone())
                .modify_tx_env(|tx| {
                    tx.caller = deployer;
                    tx.transact_to = TxKind::Create;
                    tx.data = deploy_data.clone();
                    // If the constructor test case has a value, set tx.value
                    if let Some(ref test_case) = constructor_test_case {
                        tx.value = test_case.value;
                    }
                })
                .build();

            // Deploy the contract
            let deploy_out = evm.transact().map_err(|err| {
                println!("EVM transaction error: {:?}", err);
                Errors::EVMError
            })?;
            // Extract the contract address from the deployment result
            let contract_address = match deploy_out.clone().result {
                ExecutionResult::Success { output, .. } => match output {
                    Output::Create(_, Some(addr)) => addr,
                    Output::Create(_, None) => panic!("Create failed: no address returned"),
                    _ => panic!("Create failed: unexpected output type"),
                },
                ExecutionResult::Revert { output, .. } => {
                    panic!("Execution reverted during deployment: {:?}", output)
                }
                ExecutionResult::Halt { reason, .. } => {
                    panic!("Execution halted during deployment: {:?}", reason)
                }
            };


            db.commit(deploy_out.state);

            {
                let (account_info_clone, storage_entries) = {
                    let account_info = db.load_account(contract_address).unwrap();
                    let account_info_clone = account_info.info.clone();
                    let storage_entries: Vec<_> = account_info
                        .storage
                        .iter()
                        .map(|(slot, value)| (slot.clone(), value.clone()))
                        .collect();
                    (account_info_clone, storage_entries)
                }; 
                db.insert_account_info(env_contract_address, account_info_clone);

                for (slot, value) in storage_entries {
                    let _ = db.insert_account_storage(env_contract_address, slot, value);
                }
            }

            // Now, loop over the test cases, excluding the constructor test case if it was used
            let test_cases_to_process = semantic_tests
                .test_cases
                .iter()
                .filter(|test_case| !test_case.is_constructor);

            for test_case in test_cases_to_process {
                println!("test_case: {:?}", test_case.function_name);
                let mut evm = Evm::builder()
                    .with_db(db.clone())
                    .modify_tx_env(|tx| {
                        tx.caller = "0x0000000000000000000000000000000000000001"
                            .parse()
                            .unwrap();
                        tx.transact_to = TxKind::Call(env_contract_address);
                        tx.data = test_case.input_data.clone();
                        tx.value = test_case.value;
                        tx.blob_hashes = blob_hashes.clone();
                        tx.max_fee_per_blob_gas = Some(max_blob_fee);
                    })
                .modify_env(|env| {
                    env.block.prevrandao = Some(block_prevrando);
                    env.block.difficulty = block_difficulty.into();
                })
                    .build();

                // Run the test transaction and either trace or log results
                let out = if self.trace {
                    // Use pseudocode for tracing logic
                    let mut evm = evm
                        .modify()
                        .reset_handler_with_external_context(TracerEip3155::new(
                                Box::new(std::io::stdout()),
                            ))
                        .append_handler_register(inspector_handle_register)
                        .build();

                    evm.transact().map_err(|err| {
                    println!("EVM transaction error: {:?}", err);
                    Errors::EVMError
                    })?
                } else {
                    evm.transact().map_err(|err| {
                    println!("EVM transaction error: {:?}", err);
                    Errors::EVMError
                    })?
                };

                let success_res = match out.clone().result {
                    ExecutionResult::Success { output, .. } => match output {
                        Output::Call(out) => Bytes::from(out),
                        _ => panic!("Call failed: unexpected output type"),
                    },
                    ExecutionResult::Revert { output, .. } => {
                        //Padding output to 32 bytes as there is an edge case where they do test
                        //for it using false as expected outputs, but return an empty array => 0x
                        println!("Execution reverted: {:?}", output);
                        Bytes::from(U256::ZERO.to_be_bytes::<32>())
                    }
                    ExecutionResult::Halt { reason, .. } => {
                        panic!("Execution halted during deployment: {:?}", reason)
                    }
                };

                // Verify the output matches the expected output
                assert_eq!(
                    success_res,
                    test_case.expected_outputs
                );

                // Commit the state changes from the test transaction to the database
                db.commit(out.state);
            }
        }

        Err(Errors::UnhandledTestFormat) => {
            continue;
        }
        Err(e) => {
            // Handle other errors (if any)
            return Err(e);
        }
    }
}

Ok(())
    }}
