use revm::{
    db::BenchmarkDB,
    inspector_handle_register,
    inspectors::TracerEip3155,
    primitives::{Address, Bytecode, BytecodeDecodeError, TxKind},
    Evm,
};
use std::{io::Error as IoError, path::Path, str::FromStr};
use std::path::PathBuf;
use std::fs;
use structopt::StructOpt;

extern crate alloc;

use alloy_primitives::{eip191_hash_message, keccak256, Bytes};

#[derive(Debug, thiserror::Error)]
pub enum Errors {
    #[error("The specified path does not exist")]
    PathNotExists,
    #[error("Invalid bytecode")]
    InvalidBytecode,
    #[error("Invalid input")]
    InvalidInput,
    #[error("EVM Error")]
    EVMError,
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    BytecodeDecodeError(#[from] BytecodeDecodeError),
    #[error("Invalid Test Format")]
    InvalidTestFormat,
    #[error("Invalid function signature")]
    InvalidFunctionSignature,
    #[error("Invalid Test Output")]
    InvalidTestOutput,
}

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
            let parent_dir = current_dir.parent().ok_or(Errors::PathNotExists)?;
            let semantic_tests_path = parent_dir.join("seismic-solidity-news/test/libsolidity/semanticTests/");
            find_test_files(&semantic_tests_path)?
        };

        for test_file in test_files {
            let test_file_path = test_file.to_str().ok_or(Errors::InvalidTestFormat)?;
            let (source_code, test_cases) = parse_test_file(test_file_path)?;
        }


        Ok(())
    }
}

fn find_test_files(dir: &Path) -> Result<Vec<PathBuf>, Errors> {
    let mut test_files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            // Recursively search subdirectories
            test_files.extend(find_test_files(&path)?);
        } else if let Some(extension) = path.extension() {
            if extension == "sol" {
                test_files.push(path);
            }
        }
    }
    Ok(test_files)
}

fn parse_test_file(path: &str) -> Result<(String, Vec<TestCase>), Errors> {
    let content = fs::read_to_string(path)?;
    let parts: Vec<&str> = content.split("// ----").collect();

    if parts.len() < 2 {
        return Err(Errors::InvalidTestFormat);
    }

    let source_code = parts[0];
    let expectations = parts[1].to_string();

    let test_cases = parse_calls_and_expectations(expectations)?;

    Ok((source_code.to_string(), test_cases))
}

struct TestCase {
    input_data: Bytes,
    expected_output: Bytes,
}

fn parse_calls_and_expectations(expectations: String) -> Result<Vec<TestCase>, Errors> {
    let mut test_cases = Vec::new();

    for line in expectations.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Example: "add(uint256,uint256): 1,2 -> 3"
        let parts: Vec<&str> = line.split("->").collect();
        if parts.len() != 2 {
            continue;
        }

        let call_part = parts[0].trim();
        let expected_output_part = parts[1].trim();

        let signature_and_args: Vec<&str> = call_part.split(':').collect();
        if signature_and_args.len() != 2 {
            continue;
        }

        let function_signature = hex::decode(signature_and_args[0].trim())
            .map_err(|_| Errors::InvalidFunctionSignature)?;

        let args: Vec<Bytes> = signature_and_args[1]
            .trim()
            .split(',')
            .map(|arg| Bytes::copy_from_slice(arg.trim().as_bytes()))
            .collect();

        let mut input_data = Vec::new();
        input_data.extend_from_slice(&function_signature);
        input_data.extend(args.iter().flat_map(|arg| arg));

        let expected_output = Bytes::from_str(expected_output_part)
            .map_err(|_| Errors::InvalidTestOutput)?;

        test_cases.push(TestCase {
            input_data: input_data.into(), 
            expected_output,
        });
    }

    Ok(test_cases)
}

