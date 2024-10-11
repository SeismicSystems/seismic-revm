use revm::{
    db::BenchmarkDB,
    inspector_handle_register,
    inspectors::TracerEip3155,
    primitives::{Address, Bytecode, BytecodeDecodeError, FixedBytes, TxKind},
    Evm,
};
use std::{io::Error as IoError, path::Path, str::FromStr};
use std::path::PathBuf;
use std::fs;
use structopt::StructOpt;

extern crate alloc;

use alloy_primitives::{eip191_hash_message, keccak256, Bytes, I256, U256};

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
    #[error("Invalid Argument Format")]
    InvalidArgumentFormat,
    #[error("Invalid Argument Count given Function Signature")]
    InvalidArgumentCount,
}


const SKIP_KEYWORD: [&str; 5] = ["gas", "wei", "emit", "Library", "FAILURE"];
const SKIP_DIRECTORY: [&str; 2] = ["externalContracts", "externalSource"];

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
            let should_skip = SKIP_DIRECTORY.iter().any(|&dir| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.ends_with(dir))
                    .unwrap_or(false)
            });

            if should_skip {
                continue; // Skip further processing for this directory.
            }
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
    //println!("len parts : {:?}", parts.len());
    if parts.len() != 2 {
        return Err(Errors::InvalidTestFormat);
    }
   
    let source_code = parts[0];
    let expectations = parts[1].to_string();

    let test_cases = parse_calls_and_expectations(expectations)?;

    Ok((source_code.to_string(), test_cases))
}

struct TestCase {
    input_data: Bytes,
    expected_outputs: Vec<Bytes>,
}

fn parse_calls_and_expectations(expectations: String) -> Result<Vec<TestCase>, Errors> {
    let mut test_cases = Vec::new();
    for line in expectations.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let line = if line.starts_with("//") {
            line.trim_start_matches("//").trim()
        } else {
            line
        };

        // Example: "add(uint256,uint256): 1,2 -> 3"
        let parts: Vec<&str> = line.split("->").collect();
        if parts.len() <= 2 {
            continue;
        }

        let should_skip = SKIP_KEYWORD.iter().any(|&keyword| {
            parts.iter().any(|&part| part.contains(keyword))
        });

        if should_skip {
            continue; // Skip further processing for this line.
        }

        let call_part = parts[0].trim();
        let expected_output_part = parts[1].trim();

        let signature_and_args: Vec<&str> = call_part.split(':').collect();
        if signature_and_args.len() != 2 {
            continue;
        }

        let (function_selector, parameter_types) = parse_function_signature(signature_and_args[0].trim())?;

        let args_list: Vec<&str> = signature_and_args[1].trim().split(',').map(|arg| arg.trim()).collect();
        //println!("args_list : {:?}", args_list);
        //println!("parameter_types : {:?}", parameter_types);

        if args_list.len() != parameter_types.len() {
            return Err(Errors::InvalidArgumentCount);
        }

        let mut args_encoded = Vec::new();
        for (arg_str, param_type) in args_list.iter().zip(parameter_types.iter()) {
            let arg_encoded = parse_arg(arg_str, param_type)?;
            args_encoded.push(arg_encoded);
        }

        let mut input_data = Vec::new();
        input_data.extend_from_slice(&function_selector);
        for arg in args_encoded {
            input_data.extend_from_slice(&arg);
        }

        let expected_outputs_list: Vec<&str> = expected_output_part.split(',').map(|s| s.trim()).collect();

        let mut expected_outputs = Vec::new();
        for output_arg in expected_outputs_list {
            let output_encoded = parse_output_arg(output_arg)?;
            expected_outputs.push(output_encoded);
        }

        test_cases.push(TestCase {
            input_data: input_data.into(), 
            expected_outputs,
        });
    }

    Ok(test_cases)
}

fn parse_function_signature(signature: &str) -> Result<(Vec<u8>, Vec<String>), Errors> {
    // Function signature is in the format: functionName(type1,type2,...)
    if let Some(start_idx) = signature.find('(') {
        if let Some(end_idx) = signature.rfind(')') {
            let _function_name = &signature[..start_idx];
            let params_str = &signature[start_idx + 1..end_idx];
            let parameter_types = if params_str.is_empty() {
                Vec::new()
            } else {
                params_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect()
            };
            Ok((keccak256(signature).0[0..4].to_vec(), parameter_types))
        } else {
            Err(Errors::InvalidFunctionSignature)
        }
    } else {
        Err(Errors::InvalidFunctionSignature)
    }
}

fn parse_arg(arg: &str, param_type: &str) -> Result<Bytes, Errors> {
    let arg = arg.trim();

    if param_type == "bool" {
        if arg == "true" || arg == "false" {
            let value = if arg == "true" { 1u8 } else { 0u8 };
            let mut buf = [0u8; 32];
            buf[31] = value; 
            Ok(Bytes::from(buf.to_vec()))
        } else {
            Err(Errors::InvalidArgumentFormat)
        }
    } else if param_type.starts_with("uint") {
        //Don't know why they did that, having uint in func sig but passing in a int
        if arg.starts_with("-") {
            let num = I256::from_str(arg).map_err(|_| Errors::InvalidArgumentFormat)?;
            let num_bytes = num.to_be_bytes::<32>(); 
            Ok(Bytes::from(Vec::from(num_bytes.as_slice())))    
        }
        else {
            let num = U256::from_str(arg).map_err(|_| Errors::InvalidArgumentFormat)?;
            let num_bytes = num.to_be_bytes::<32>(); 
            Ok(Bytes::from(Vec::from(num_bytes.as_slice())))    
        }
    } else if param_type.starts_with("int") {
        let num = I256::from_str(arg).map_err(|_| Errors::InvalidArgumentFormat)?;
        let num_bytes = num.to_be_bytes::<32>(); 
        Ok(Bytes::from(Vec::from(num_bytes.as_slice())))    
    } else if param_type.starts_with("bytes") {
        if arg.starts_with("left(") && arg.ends_with(')') {
            let inner = &arg[5..arg.len() - 1];
            let bytes = Bytes::from_str(inner).map_err(|_| Errors::InvalidArgumentFormat)?;
            let mut padded = bytes.to_vec();

            if padded.len() < 32 {
                padded.resize(32, 0);
            }
            Ok(Bytes::from(padded))
        } else if arg.starts_with("right(") && arg.ends_with(')') {
            let inner = &arg[6..arg.len() - 1];
            let bytes = Bytes::from_str(inner).map_err(|_| Errors::InvalidArgumentFormat)?;
            let mut padded = vec![0u8; 32];

            // Left-pad with zeroes to ensure it's 32 bytes long.
            let bytes_len = bytes.len();
            if bytes_len <= 32 {
                padded[32 - bytes_len..].copy_from_slice(&bytes);
            } else {
                return Err(Errors::InvalidArgumentFormat);
            }
            Ok(Bytes::from(padded))
        } else if arg.starts_with("0x") {
            Ok(Bytes::from_str(arg).map_err(|_| Errors::InvalidArgumentFormat)?)
        } else {
            Err(Errors::InvalidArgumentFormat)
        }
    } else {
        // Default case for unsupported parameter types
        Err(Errors::InvalidArgumentFormat)
    }
}

    fn parse_output_arg(arg: &str) -> Result<Bytes, Errors> {
        let arg = arg.trim();

        if arg == "true" || arg == "false" {
            let value = if arg == "true" { 1u8 } else { 0u8 };
            let mut buf = [0u8; 32];
            buf[31] = value; 
            return Ok(Bytes::from(buf.to_vec()));
        }

        if arg.starts_with("-") {
            let num = I256::from_str(arg).map_err(|_| Errors::InvalidArgumentFormat)?;
            let num_bytes = num.to_be_bytes::<32>(); 
            return Ok(Bytes::from(Vec::from(num_bytes.as_slice())))    
        }

        if let Ok(num) = U256::from_str(arg) {
            let num_bytes = num.to_be_bytes::<32>(); 
            return Ok(Bytes::from(Vec::from(num_bytes.as_slice())))    
        }


        // Handle hex values
        if arg.starts_with("0x") {
            let bytes = hex::decode(arg.trim_start_matches("0x"))
                .map_err(|_| Errors::InvalidArgumentFormat)?;
            return Ok(Bytes::from(bytes));
        }

        if arg.starts_with("left(") && arg.ends_with(')') {
            let inner = &arg[5..arg.len() - 1];
            let bytes = hex::decode(inner.trim_start_matches("0x"))
                .map_err(|_| Errors::InvalidArgumentFormat)?;
            // For left(), pad on the right with zeros to 32 bytes
            let mut buf = Vec::from(bytes);
            if buf.len() < 32 {
                buf.resize(32, 0u8);
            }
            return Ok(Bytes::from(buf));
        } else if arg.starts_with("right(") && arg.ends_with(')') {
            let inner = &arg[6..arg.len() - 1];
            let bytes = hex::decode(inner.trim_start_matches("0x"))
                .map_err(|_| Errors::InvalidArgumentFormat)?;
            let mut buf = vec![0u8; 32];
            let len = bytes.len();
            if len > 32 {
                return Err(Errors::InvalidArgumentFormat);
            }
            buf[32 - len..].copy_from_slice(&bytes);
            return Ok(Bytes::from(buf));
        }

        // If none of the above, return error
        Err(Errors::InvalidArgumentFormat)
    }

