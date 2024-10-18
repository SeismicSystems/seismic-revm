use evm_handler::{EvmConfig, EvmExecutor};
use revm::{
    db::{CacheDB, EmptyDB},
    primitives::{AccountInfo, Bytes, U256},
};
use test_cases::TestCase;

use std::path::PathBuf;
use structopt::StructOpt;

extern crate alloc;

mod errors;
pub use errors::Errors;
mod semantic_tests;
use semantic_tests::SemanticTests;
mod compiler_evm_versions;
mod evm_handler;
mod parser;
mod test_cases;
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
        let test_files = self.find_test_files()?;

        for test_file in test_files {
            println!("test_file: {:?}", test_file);
            let test_file_path = test_file.to_str().ok_or(Errors::InvalidTestFormat)?;

            match SemanticTests::new(test_file_path) {
                Ok(semantic_tests) => {
                    let evm_version = semantic_tests.contract_infos[0].evm_version;
                    let mut evm_config = EvmConfig::new(evm_version);
                    let mut db = self.prepare_database(&evm_config)?;

                    let constructor_test_case = semantic_tests
                        .test_cases
                        .iter()
                        .find(|test_case| test_case.is_constructor)
                        .cloned();

                    let deploy_data =
                        self.prepare_deploy_data(&semantic_tests, &constructor_test_case)?;
                    if deploy_data.is_empty() {
                        continue;
                    }
                    let mut evm_executor =
                        EvmExecutor::new(db, evm_config.clone(), evm_version, &semantic_tests);

                    let contract_address = evm_executor.deploy_contract(
                        deploy_data,
                        constructor_test_case
                            .as_ref()
                            .map_or(U256::ZERO, |tc| tc.value),
                    )?;
                    evm_executor.config.block_number =
                        evm_executor.config.block_number.wrapping_add(U256::from(1));
                    evm_executor.copy_contract_to_env(contract_address);

                    let test_cases_to_process = semantic_tests
                        .test_cases
                        .iter()
                        .filter(|test_case| !test_case.is_constructor);

                    for test_case in test_cases_to_process {
                        evm_executor.run_test_case(test_case)?;
                        evm_executor.config.block_number =
                            evm_executor.config.block_number.wrapping_add(U256::from(1));
                    }
                }
                Err(Errors::UnhandledTestFormat) => {
                    continue;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    fn find_test_files(&self) -> Result<Vec<PathBuf>, Errors> {
        if let Some(ref path) = self.path {
            if path.is_file() {
                Ok(vec![path.clone()])
            } else if path.is_dir() {
                find_test_files(path)
            } else {
                Err(Errors::PathNotExists)
            }
        } else {
            let current_dir = std::env::current_dir()?;
            let parent_dir = current_dir
                .parent()
                .ok_or(Errors::PathNotExists)?
                .parent()
                .ok_or(Errors::PathNotExists)?
                .parent()
                .ok_or(Errors::PathNotExists)?;
            let semantic_tests_path =
                parent_dir.join("seismic-solidity-new/test/libsolidity/semanticTests/");
            find_test_files(&semantic_tests_path)
        }
    }

    fn prepare_database(&self, config: &EvmConfig) -> Result<CacheDB<EmptyDB>, Errors> {
        let mut db = CacheDB::new(EmptyDB::default());
        let account_info = AccountInfo {
            balance: U256::MAX,
            nonce: Default::default(),
            code: None,
            code_hash: Default::default(),
        };
        db.insert_account_info(config.caller, account_info);
        Ok(db)
    }

    fn prepare_deploy_data(
        &self,
        semantic_tests: &SemanticTests,
        constructor_test_case: &Option<TestCase>,
    ) -> Result<Bytes, Errors> {
        if let Some(ref test_case) = constructor_test_case {
            Ok(test_case.deploy_binary.clone())
        } else if let Some(first_test_case) = semantic_tests.test_cases.first() {
            Ok(first_test_case.deploy_binary.clone())
        } else {
            Ok(Bytes::new())
        }
    }
}
