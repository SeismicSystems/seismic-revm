use std::str::FromStr;

use super::{errors::Errors, semantic_tests::ContractInfo};
use alloy_primitives::{keccak256, I256, U256};
use revm::primitives::{Bytes, FixedBytes};

const SKIP_KEYWORD: [&str; 5] = ["gas", "wei", "emit", "Library", "FAILURE"];

#[derive(Debug)]
pub(crate) struct TestCase {
    pub function_name: String,
    pub input_data: Bytes,
    pub expected_outputs: Bytes,
    pub is_constructor: bool,
    pub contract_binary: Bytes,
}

impl TestCase {
    pub(crate) fn from_expectations(expectations: String,
        contract_infos: &[ContractInfo]) -> Result<Vec<Self>, Errors> {
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
            if parts.len() < 2  || parts.len() > 3 {
                continue;
            }

            let should_skip = SKIP_KEYWORD.iter().any(|&keyword| {
                parts.iter().any(|&part| part.contains(keyword))
            });

            if should_skip {
                continue; 
            }

            let call_part = parts[0].trim();
            let expected_output_part = parts[1].trim();

            let signature_and_args: Vec<&str> = call_part.split(':').collect();
            if signature_and_args.len() > 2 {
                continue;
            }
            
            let mut is_constructor = false;

            if signature_and_args[0].starts_with("constructor(") {
                is_constructor = true
            }
            
            let (function_selector, parameter_types) = Self::parse_function_signature(signature_and_args[0].trim())?;
            let args_list = if signature_and_args.len() > 1 && !signature_and_args[1].trim().is_empty() {
                signature_and_args[1].trim().split(',').map(|arg| arg.trim()).collect::<Vec<&str>>()
            } else {
                Vec::new() 
            };

            if args_list.len() != parameter_types.len() {
                return Err(Errors::InvalidArgumentCount);
            }

            let mut args_encoded = Vec::new();
            for (arg_str, param_type) in args_list.iter().zip(parameter_types.iter()) {
                let arg_encoded = Self::parse_arg(arg_str, param_type)?;
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
                let output_encoded = Self::parse_output_arg(output_arg)?;
                expected_outputs.extend_from_slice(output_encoded.as_ref());
            }

            let matching_contract = contract_infos.iter().find(|contract| {
                contract.has_function(signature_and_args[0].trim())
            });

            if let Some(contract) = matching_contract {
                let binary = if is_constructor {
                    contract.compile_binary.clone().unwrap()
                } else {
                    contract.runtime_binary.clone()
                };

                test_cases.push(TestCase {
                    function_name: signature_and_args[0].to_string(), 
                    input_data: input_data.into(), 
                    expected_outputs: Bytes::from(expected_outputs),
                    is_constructor,
                    contract_binary: binary,  
                });
            } else {
                // Handle case where no matching contract is found (optional)
                println!("No matching contract found for function: {}", signature_and_args[0].trim());
            }
        }
        Ok(test_cases)
    }

    fn parse_function_signature(signature: &str) -> Result<(Vec<u8>, Vec<String>), Errors> {
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
            println!("Unsupported parameter type: {:?}", arg);
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
            return Self::parse_output_arg(inner) 
        } else if arg.starts_with("right(") && arg.ends_with(')') {
            let inner = &arg[6..arg.len() - 1];
            return Self::parse_output_arg(inner) 
        }
        else if arg.starts_with("\"") && arg.ends_with("\"") {
            let inner = &arg[1..arg.len() - 1];
            let string_bytes = inner.as_bytes().to_vec();
            let output = FixedBytes::<32>::right_padding_from(string_bytes.as_ref());
            return Ok(Bytes::from(output.to_vec()));
        }
        // If none of the above, return error
        Err(Errors::InvalidArgumentFormat)
    }
}


