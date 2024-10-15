use revm::{
    db::BenchmarkDB,
    inspector_handle_register,
    inspectors::TracerEip3155,
    primitives::{Address, Bytecode, BytecodeDecodeError, TxKind},
    Evm,
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
                        if !test_case.is_constructor {
                            let mut evm = Evm::builder()
                                .with_db(BenchmarkDB::new_bytecode(Bytecode::new_raw(
                                    test_case.contract_binary.clone(),
                                )))
                                .modify_tx_env(|tx| {
                                    tx.caller = "0x0000000000000000000000000000000000000001"
                                        .parse()
                                        .unwrap();
                                    tx.transact_to = TxKind::Call(Address::ZERO);
                                    tx.data = test_case.input_data.clone(); 
                                })
                                .build();

                            println!("test_case.input_data: {:?}", test_case.input_data);
                            println!("evm.env.tx.data: {:?}", evm.context.evm.env.tx.data);

                            // Run the transaction and either trace or log results
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
                                println!("out.result {:?}", out);
                                assert_eq!(out.result.output().unwrap(), &test_case.expected_outputs);

                                // You might want to process the output here, e.g., validate it against expected outputs.
                                // Compare out.result with test_case.expected_outputs
                            }
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
