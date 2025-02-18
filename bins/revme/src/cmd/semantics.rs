use evm_handler::{EvmConfig, EvmExecutor};
use revm::{
    db::{CacheDB, EmptyDB}, primitives::{AccountInfo, Bytes, SpecId, U256}, CacheState, State
};
use test_cases::TestCase;

use log::{debug, info, LevelFilter};
use rayon::prelude::*;
use std::path::PathBuf;
use std::time::Instant;
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
    /// Increase output verbosity. Can be used multiple times. For example `-vvv` will set the log level to `TRACE`.
    #[structopt(short, long, parse(from_occurrences))]
    verbose: usize,
    /// Run tests in a single thread.
    #[structopt(short = "s", long)]
    single_thread: bool,
    /// Will not return on failure.
    #[structopt(long, alias = "no-fail-fast")]
    keep_going: bool,
}

impl Cmd {
    pub fn run(&self) -> Result<(), Errors> {
        self.setup_logging();
        let start_time = Instant::now();
        let test_files = self.find_test_files()?;

        if self.single_thread {
            info!("Running in single-threaded mode");

            for test_file in test_files {
                self.process_test_file(test_file)?;
            }
        } else {
            info!("Running in multi-threaded mode");

            // Use parallel iterator
            test_files
                .par_iter()
                .try_for_each(|test_file| self.process_test_file(test_file.clone()))?;
        }

        let duration = start_time.elapsed();
        info!("Execution time: {:?}", duration);

        Ok(())
    }

    fn setup_logging(&self) {
        let log_level = match self.verbose {
            0 => LevelFilter::Warn,
            1 => LevelFilter::Info,
            2 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        };

        env_logger::Builder::new().filter_level(log_level).init();
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

    fn process_test_file(&self, test_file: PathBuf) -> Result<(), Errors> {
        info!("test_file: {:?}", test_file);
        let test_file_path = test_file.to_str().ok_or(Errors::InvalidTestFormat)?;

        match SemanticTests::new(test_file_path) {
            Ok(semantic_tests) => {
                let evm_version = semantic_tests.contract_infos[0].evm_version;
                let evm_config = EvmConfig::new(evm_version);
                let cache = self.prepare_database(&evm_config)?;

                let constructor_test_case = semantic_tests
                    .test_cases
                    .iter()
                    .find(|test_case| test_case.is_constructor)
                    .cloned();

                let deploy_data =
                    self.prepare_deploy_data(&semantic_tests, &constructor_test_case)?;
                let mut evm_executor = EvmExecutor::new(cache, evm_config.clone(), evm_version);

                debug!("constructor test_case: {:?}", constructor_test_case);

                let contract_address = evm_executor.deploy_contract(
                    deploy_data,
                    constructor_test_case.unwrap_or_default(),
                    self.trace,
                )?;
                evm_executor.config.block_number =
                    evm_executor.config.block_number.wrapping_add(U256::from(1));
                evm_executor.copy_contract_to_env(contract_address);

                let test_cases_to_process = semantic_tests
                    .test_cases
                    .iter()
                    .filter(|test_case| !test_case.is_constructor);

                for test_case in test_cases_to_process {
                    let result = evm_executor.run_test_case(test_case, self.trace, test_file_path);
                    match result {
                        Ok(_) => {}
                        Err(e) => {
                            if !self.keep_going {
                                return Err(e);
                            }
                        }
                    };
                    evm_executor.config.block_number =
                        evm_executor.config.block_number.wrapping_add(U256::from(1));
                }
            }
            Err(Errors::UnhandledTestFormat) => {
                return Ok(());
            }
            Err(e) => {
                return Err(e);
            }
        }
        Ok(())
    }

    fn prepare_database(&self, config: &EvmConfig) -> Result<CacheState, Errors> {
        let mut cache = revm::CacheState::new(false);
        let account_info = AccountInfo {
            balance: U256::MAX,
            nonce: Default::default(),
            code: None,
            code_hash: Default::default(),
        };
        cache.insert_account(config.caller, account_info);
        cache.set_state_clear_flag(SpecId::enabled(
            config.evm_version,
            revm::primitives::SpecId::SPURIOUS_DRAGON,
        ));
        Ok(cache)
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
