use alloy_primitives::U256;
use hashbrown::HashMap;
use k256::elliptic_curve::consts::U25;
use revm::{
    db::{BenchmarkDB, CacheDB, EmptyDB}, inspector_handle_register, inspectors::TracerEip3155, primitives::{AccountInfo, Address, Bytecode, BytecodeDecodeError, ExecutionResult, Output, TxKind}, DatabaseCommit, Evm
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
                    return Err(Errors::InvalidInput);
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

            // Commit the state changes from deployment to the database
            db.commit(deploy_out.state);

            // Now, loop over the test cases, excluding the constructor test case if it was used
            let test_cases_to_process = semantic_tests
                .test_cases
                .iter()
                .filter(|test_case| !test_case.is_constructor);

            for test_case in test_cases_to_process {
                println!("test_case: {:?}", test_case);
                let mut evm = Evm::builder()
                    .with_db(db.clone())
                    .modify_tx_env(|tx| {
                        tx.caller = "0x0000000000000000000000000000000000000001"
                            .parse()
                            .unwrap();
                        tx.transact_to = TxKind::Call(contract_address);
                        tx.data = test_case.input_data.clone();
                        tx.value = test_case.value;
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

                    evm.transact().map_err(|_| Errors::EVMError)?
                } else {
                    evm.transact().map_err(|_| Errors::EVMError)?
                };

                println!("Transaction result: {:?}", out.result);

                // Verify the output matches the expected output
                assert_eq!(
                    out.result.output().unwrap(),
                    &test_case.expected_outputs
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
