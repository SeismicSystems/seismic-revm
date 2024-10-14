use revm::{
    db::BenchmarkDB,
    inspector_handle_register,
    inspectors::TracerEip3155,
    primitives::{Address, Bytecode, BytecodeDecodeError, FixedBytes, TxKind},
    Evm,
};

use std::path::PathBuf;
use structopt::StructOpt;

extern crate alloc;

mod errors;
pub use errors::Errors;
mod file_handler;
use file_handler::{find_test_files, parse_test_file};
mod test_cases;
mod compiler_evm_versions;

/// EVM runner command that allows running Solidity semantic tests.
/// If a path is provided, it will process that file or recursively process all `.sol` files in that directory.
/// If no path is provided, it defaults to the Solidity semantic tests directory.
#[derive(StructOpt, Debug)]
pub struct Cmd {
    /// Path to a Solidity file or directory containing Solidity files. If no file is provided, it will default to the Solidity semantic tests directory.
    #[structopt(long)]
    path: Option<PathBuf>
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
            let test_file_path = test_file.to_str().ok_or(Errors::InvalidTestFormat)?;
            match parse_test_file(test_file_path) {
                Ok((source_code, test_cases)) => {
                    println!("Running test cases for file {:?}", test_file_path);
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
    }
}
