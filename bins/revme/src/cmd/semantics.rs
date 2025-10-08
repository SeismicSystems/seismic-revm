use cargo_metadata::MetadataCommand;
use evm_handler::{EvmConfig, EvmExecutor};
use revm::{
    database::{CacheDB, EmptyDB},
    primitives::U256,
};

use log::{error, info, LevelFilter};
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

use crate::cmd::semantics::test_cases::TestCase;

/// EVM runner command that allows running Solidity semantic tests.
/// If a path is provided, it will process that file or recursively process all `.sol` files in that directory.
/// If no path is provided, it defaults to the Solidity semantic tests directory.
#[derive(Parser, Debug)]
pub struct Cmd {
    /// Path to a Solidity file or directory containing Solidity files. If no file is provided,
    /// it will default to the Solidity semantic tests directory.
    #[clap(short = 't', long, alias = "tests")]
    tests_path: Option<PathBuf>,

    /// Path to seismic-solidity executable
    #[clap(short = 's', long, alias = "ssolc", default_value_t = String::from("/usr/local/bin/solc"))]
    ssolc_path: String,

    /// Print the trace.
    #[clap(long)]
    trace: bool,

    /// Increase output verbosity. Can be used multiple times. For example `-vvv` will set the log level to `TRACE`.
    #[clap(short, long, action = ArgAction::Count)]
    verbose: u8,

    /// Run tests in a single thread.
    #[clap(short = 'i', long)]
    single_thread: bool,

    /// Will not return on failure.
    #[clap(long, alias = "no-fail-fast")]
    keep_going: bool,

    /// Include tests that need EOF.
    /// Disabled by default because Mercury does not support it.
    /// Also, these don't work.
    #[clap(short, long)]
    eof: bool,
}

impl Cmd {
    pub fn run(&self) -> Result<(), Errors> {
        self.setup_logging();
        let start_time = Instant::now();
        let test_files = self.find_test_files()?;
        let n_files = test_files.len();

        let failures = match self.single_thread {
            true => {
                info!("Running in single-threaded mode");
                self.run_single_threaded(test_files)
            }
            false => {
                info!("Running in multi-threaded mode");
                test_files
                    .par_iter()
                    .filter_map(|test_file| {
                        match self.process_test_file(test_file.clone()) {
                            Err(file_failures) => Some((test_file.clone(), file_failures)),
                            Ok(_) => None, // No failures for this file
                        }
                    })
                    .collect()
            }
        };

        let duration = start_time.elapsed();
        info!("Execution time: {:?}", duration);
        if failures.len() == 0 {
            println!("All tests passed across {} files ✅", n_files);
            return Ok(());
        }
        let total_failures: usize = failures
            .iter()
            .map(|(_, file_failures)| file_failures.len())
            .sum();

        println!(
            "❌ {} test(s) failed across {}/{} file(s):\n",
            total_failures,
            failures.len(),
            n_files,
        );
        let test_parent = self.tests_parent_folder()?;
        for (test_file, file_failures) in failures {
            let relative_path = test_file.strip_prefix(&test_parent).unwrap_or(&test_file);
            let mut output = format!("📁 {}", relative_path.display());

            for (error, test_case_opt) in file_failures {
                let error_line = match error {
                    Errors::EVMError(s) => format!("  ⚡ EVM execution error: {}", s),
                    Errors::LogMismatch => "  ⚠️  Event log mismatch".into(),
                    Errors::BalanceMismatch => "  💰 Balance assertion failed".into(),
                    Errors::StorageMismatch => "  🗃️  Storage state mismatch".into(),
                    Errors::InvalidBytecode => "  🔧 Invalid contract bytecode".into(),
                    Errors::CompilationFailed => "  🛠️  Contract compilation failed".into(),
                    Errors::CompilerNotFound => "  ❗ Solc compiler not found".into(),
                    _ => {
                        output.push_str(&format!("\n  ❌ error: {}", error));
                        continue;
                    }
                };

                output.push_str(&format!("\n{}", error_line));

                if self.verbose > 0 {
                    if let Some(test_case) = test_case_opt {
                        output.push_str(&format!("\n{:#?}", test_case.steps));
                    }
                }
            }

            error!("{}", output);
        }

        Err(Errors::TestsFailed)
    }

    fn run_single_threaded(
        &self,
        test_files: Vec<PathBuf>,
    ) -> Vec<(PathBuf, Vec<(Errors, Option<TestCase>)>)> {
        let mut failures = vec![];
        for test_file in test_files {
            if let Err(file_failures) = self.process_test_file(test_file.clone()) {
                failures.push((test_file, file_failures));
                if !self.keep_going {
                    return failures;
                }
            }
        }
        failures
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

    fn default_parent_dir() -> Result<PathBuf, Errors> {
        let workspace_root: PathBuf = MetadataCommand::new().exec()
            .expect("Failed to detect workspace root. Use -p to provide a path to solidity test directory")
            .workspace_root.into();
        let parent_dir = workspace_root.parent().ok_or(Errors::PathNotExists)?;
        Ok(parent_dir.into())
    }

    fn default_tests_path() -> Result<PathBuf, Errors> {
        return Ok(
            Self::default_parent_dir()?.join("seismic-solidity/test/libsolidity/semanticTests/")
        );
    }

    fn tests_parent_folder(&self) -> Result<PathBuf, Errors> {
        match &self.tests_path {
            Some(p) => Ok(p.clone()),
            None => Ok(Self::default_tests_path()?),
        }
    }

    fn find_test_files(&self) -> Result<Vec<PathBuf>, Errors> {
        match &self.tests_path {
            Some(path) => {
                if path.is_file() {
                    Ok(vec![path.clone()])
                } else if path.is_dir() {
                    find_test_files(&path)
                } else {
                    Err(Errors::PathNotExists)
                }
            }
            None => find_test_files(&Self::default_tests_path()?),
        }
    }

    fn process_test_file(&self, test_file: PathBuf) -> Result<(), Vec<(Errors, Option<TestCase>)>> {
        info!("test_file: {:?}", test_file);
        let test_file_path = match test_file.to_str() {
            Some(p) => p,
            None => return Err(Errors::InvalidTestFormat.into()),
        };

        let failures = match SemanticTests::new(test_file_path, &self.ssolc_path, !self.eof) {
            Ok(semantic_tests) => {
                let evm_version = semantic_tests.contract_infos[0].evm_version;
                let evm_config = EvmConfig::new(evm_version);
                let db = self
                    .prepare_database(&evm_config)
                    .map_err(|e| vec![(e, None)])?;

                let mut evm_executor = EvmExecutor::new(db, evm_config.clone(), evm_version);

                let mut failures = vec![];
                for test_case in &semantic_tests.test_cases {
                    let result = evm_executor.run_test_case(test_case, self.trace, test_file_path);
                    match result {
                        Ok(_) => {}
                        Err(e) => {
                            let case_err = (e, Some(test_case.clone()));
                            match self.keep_going {
                                true => {
                                    failures.push(case_err);
                                }
                                false => {
                                    return Err(vec![case_err]);
                                }
                            }
                        }
                    };
                    evm_executor.config.block_number =
                        evm_executor.config.block_number.wrapping_add(U256::from(1));

                    // Modification: timestamp of block will be in milliseconds since the UNIX epoch
                    evm_executor.config.timestamp = evm_executor
                        .config
                        .timestamp
                        .wrapping_add(U256::from(15000));
                }
                failures
            }
            Err(Errors::UnhandledTestFormat) => {
                return Ok(());
            }
            Err(e) => {
                return Err(e.into());
            }
        };
        match failures.len() {
            0 => Ok(()),
            _ => Err(failures),
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
}
