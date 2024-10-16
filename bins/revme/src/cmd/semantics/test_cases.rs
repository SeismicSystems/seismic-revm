use std::str::FromStr;

use super::{errors::Errors, semantic_tests::ContractInfo, parser::Parser};
use alloy_primitives::U256;
use revm::primitives::Bytes;

const SKIP_KEYWORD: [&str; 4] = ["gas", "emit", "Library", "FAILURE"];

#[derive(Debug, Clone)]
pub(crate) struct TestCase {
    pub function_name: String,
    pub input_data: Bytes,
    pub expected_outputs: Bytes,
    pub is_constructor: bool,
    pub deploy_binary: Bytes,
    pub value: U256,
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
            // constructor(), 2 wei: 3 ->"
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
            
            let mut value = U256::ZERO;
            let signature_and_args_temp: Vec<&str> = call_part.split(':').collect();
            let signature: Vec<_> = signature_and_args_temp[0].trim().split(",").collect();
            let mut signature_and_args: Vec<&str> = Vec::new();

            if signature.len() == 2 {
                let string_value = signature[1].trim().split(" ").collect::<Vec<_>>()[0];
                value = U256::from_str(string_value).map_err(|_| Errors::InvalidInput)?;
                signature_and_args.push(signature[0]);
                signature_and_args.push(signature_and_args_temp[1]);
            }
            else {
                signature_and_args = signature_and_args_temp;
            }

            let (function_selector, parameter_types) = Parser::parse_function_signature(signature_and_args[0].trim())?;
            
            let args_list = if signature_and_args.len() > 1 && !signature_and_args[1].trim().is_empty() {
                signature_and_args[1].trim().split(',').map(|arg| arg.trim()).collect::<Vec<&str>>()
            } else {
                Vec::new() 
            };
            
            let mut is_constructor = false;
            if signature_and_args[0].starts_with("constructor(") {
                is_constructor = true
            }

            if args_list.len() != parameter_types.len() {
                return Err(Errors::InvalidArgumentCount);
            }

            let mut args_encoded = Vec::new();
            for (arg_str, param_type) in args_list.iter().zip(parameter_types.iter()) {
                let arg_encoded = Parser::parse_arg(arg_str, param_type)?;
                args_encoded.push(arg_encoded);
            }

            let mut input_data = Vec::new();
            input_data.extend_from_slice(&function_selector);
            for arg in args_encoded.clone() {
                input_data.extend_from_slice(&arg);
            }

            let expected_outputs_list: Vec<&str> = expected_output_part.split(',').map(|s| s.trim()).collect();

            let mut expected_outputs = Vec::new();
            for output_arg in expected_outputs_list {
                let output_encoded = Parser::parse_output_arg(output_arg)?;
                expected_outputs.extend_from_slice(output_encoded.as_ref());
            }
            
            let matching_contract = contract_infos.iter().find(|contract| {
                contract.has_function(signature_and_args[0].split('(').next().unwrap_or(""))
            });

            if let Some(contract) = matching_contract {
                let mut deploy_binary = Vec::new();
                deploy_binary.extend_from_slice(&contract.compile_binary);
                if is_constructor {
                    for arg in args_encoded {
                            deploy_binary.extend_from_slice(&arg);
                    }
                        test_cases.push(TestCase {
                            function_name: signature_and_args[0].to_string(), 
                            input_data: vec![].into(), 
                            expected_outputs: Bytes::from(expected_outputs),
                            is_constructor,
                            deploy_binary: deploy_binary.into(),  
                            value,  
                    });
                } else {
                    test_cases.push(TestCase {
                        function_name: signature_and_args[0].to_string(), 
                        input_data: input_data.into(), 
                        expected_outputs: Bytes::from(expected_outputs),
                        is_constructor,
                        deploy_binary: deploy_binary.into(),  
                        value,  
                    });
                }
            } else {
                // Handle case where no matching contract is found (optional)
                println!("No matching contract found for function: {}", signature_and_args[0].trim());
            }
        }
        Ok(test_cases)
    }

}


