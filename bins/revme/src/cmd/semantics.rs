use evm_handler::{EvmConfig, EvmExecutor};
use revm::{
    database::{CacheDB, EmptyDB},
    primitives::U256,
};

use log::{info, LevelFilter};
use rayon::prelude::*;
use state::AccountInfo;
use std::path::PathBuf;
use std::time::Instant;

use clap::{ArgAction, Parser};

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
#[derive(Parser, Debug)]
pub struct Cmd {
    /// Path to a Solidity file or directory containing Solidity files. If no file is provided,
    /// it will default to the Solidity semantic tests directory.
    #[clap(long)]
    path: Option<PathBuf>,

    /// Print the trace.
    #[clap(long)]
    trace: bool,

    /// Increase output verbosity. Can be used multiple times. For example `-vvv` will set the log level to `TRACE`.
    #[clap(short, long, action = ArgAction::Count)]
    verbose: u8,

    /// Run tests in a single thread.
    #[clap(short = 's', long)]
    single_thread: bool,

    /// Will not return on failure.
    #[clap(long, alias = "no-fail-fast")]
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
                let db = self.prepare_database(&evm_config)?;

                let mut evm_executor = EvmExecutor::new(db, evm_config.clone(), evm_version);

                for test_case in &semantic_tests.test_cases {
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
                        evm_executor.config.block_number.wrapping_add(1);

                    evm_executor.config.timestamp = evm_executor.config.timestamp.wrapping_add(15);
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
}
