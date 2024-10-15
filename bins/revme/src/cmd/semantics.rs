use revm::{
    db::{BenchmarkDB, CacheDB, EmptyDB}, inspector_handle_register, inspectors::TracerEip3155, primitives::{Address, Bytecode, BytecodeDecodeError, ExecutionResult, Output, TxKind}, DatabaseCommit, Evm
};

use std::path::PathBuf;
use structopt::StructOpt;

extern crate alloc;

mod errors;
pub use errors::Errors;
mod semantic_tests;
use semantic_tests::SemanticTests;
mod test_cases;
mod compiler_evm_versions;
mod evm_handler;
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
            for test_case in semantic_tests.test_cases {
                // Create a mutable database instance to share between transactions
                let mut db = CacheDB::new(EmptyDB::default());

                // Build EVM instance for deployment with the database
                let mut evm = Evm::builder()
                    .with_db(db.clone())
                    .modify_tx_env(|tx| {
                        tx.caller = "0x0000000000000000000000000000000000000001"
                            .parse()
                            .unwrap();
                        tx.transact_to = TxKind::Create;
                        tx.data = test_case.deploy_binary.clone(); 
                    })
                    .build();

                let deploy_out = evm.transact().map_err(|_| Errors::EVMError)?;
                let contract_address = match deploy_out.result {
                    ExecutionResult::Success { output, .. } => match output {
                        Output::Create(_, Some(addr)) => addr,
                        Output::Create(_, None) => panic!("Create failed: no address returned"),
                        _ => panic!("Create failed: unexpected output type"),
                    },
                    ExecutionResult::Revert { output, .. } => panic!("Execution reverted: {:?}", output),
                    ExecutionResult::Halt { reason, .. } => panic!("Execution halted: {:?}", reason),
                };
                db.commit(deploy_out.state);
                // Now, build a new EVM instance for the test transaction with the updated database
                let mut evm = Evm::builder()
                    .with_db(db) // Reuse the database with the deployed contract
                    .modify_tx_env(|tx| {
                        tx.caller = "0x0000000000000000000000000000000000000001"
                            .parse()
                            .unwrap();
                        tx.transact_to = TxKind::Call(Address::from_slice(contract_address.as_ref()));
                        tx.data = test_case.input_data.clone();
                    })
                    .build();

                // Run the test transaction and either trace or log results
                let out = if self.trace {
                    let mut evm = evm
                        .modify()
                        .reset_handler_with_external_context(TracerEip3155::new(
                            Box::new(std::io::stdout()),
                        ))
                        .append_handler_register(inspector_handle_register)
                        .build();

                    evm.transact().map_err(|_| Errors::EVMError)?
                } else {
                    let out = evm.transact().map_err(|_| Errors::EVMError)?;
                    out
                };

                println!("test {:?}", test_case);
                println!("out.result {:?}", out.result);
                assert_eq!(
                    out.result.output().unwrap(),
                    &test_case.expected_outputs
                );

                // You might want to process the output here, e.g., validate it against expected outputs.
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
}
}
