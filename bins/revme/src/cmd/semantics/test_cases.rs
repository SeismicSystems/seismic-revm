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

    pub(crate) fn from_expectations(
    expectations: String,
    contract_infos: &[ContractInfo],
) -> Result<Vec<Self>, Errors> {
        let mut test_cases = Vec::new();

        for line in expectations.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let line = if line.starts_with("//") {
                line.trim_start_matches("//").trim()
            } else {
                line
            };

            // format:
            //function_signature "," inputs ":" inputs "->" outputs
            // Split the line into call part and expected output part
            let parts: Vec<&str> = line.split("->").collect();
            if parts.len() > 2 {
                return Err(Errors::InvalidInput);
            }

            let call_part = parts[0].trim();
            let expected_output_part = if parts.len() == 2 {
                parts[1].trim()
            } else {
                ""
            };

            let should_skip = SKIP_KEYWORD.iter().any(|&keyword| {
                call_part.contains(keyword) || expected_output_part.contains(keyword)
            });
            if should_skip {
                continue;
            }

            let (function_signature, value, inputs) = Self::parse_call_part(call_part)?;
            
            let expected_outputs = Self::parse_outputs(expected_output_part)?;
            let (function_selector, _) =
                Parser::parse_function_signature(&function_signature)?;

            let is_constructor = function_signature.starts_with("constructor(");

            let mut args_encoded = Vec::new();
            for arg_str in inputs.iter() {
                let arg_encoded = Parser::parse_output_arg(arg_str)?;
                args_encoded.push(arg_encoded);
            }
            
            let mut input_data = Vec::new();
            if !is_constructor && function_signature != "()" {
                input_data.extend_from_slice(&function_selector);
            }
            for arg in &args_encoded {
                input_data.extend_from_slice(arg);
            }

            let matching_contract = contract_infos.iter().find(|contract| {
                if function_signature == "()" {
                    contract.has_fallback_function()
                } else {
                    let function_name = function_signature
                        .split('(')
                        .next()
                        .unwrap_or("")
                        .trim();
                    contract.has_function(function_name)
                }
            });
  
            if let Some(contract) = matching_contract {
                let mut deploy_binary = Vec::new(); 
                deploy_binary.extend_from_slice(&contract.compile_binary);

                if is_constructor {
                    for arg in &args_encoded {
                        deploy_binary.extend_from_slice(&arg);
                    } 
                    input_data.clear(); // No input data for constructor call
                }

                test_cases.push(TestCase {
                    function_name: function_signature.clone(),
                    input_data: input_data.into(),
                    expected_outputs: Bytes::from(expected_outputs),
                    is_constructor,
                    deploy_binary: deploy_binary.into(),
                    value: value.unwrap_or(U256::ZERO),
                });
            } else {
                println!(
                    "No matching contract found for function: {}",
                    function_signature
                );
            }
        }

        Ok(test_cases)
    }

    fn parse_call_part(call_part: &str) -> Result<(String, Option<U256>, Vec<String>), Errors> {
        // Find the function signature by matching parentheses
        let mut paren_count = 0;
        let mut sig_end_idx = None;
        for (i, c) in call_part.char_indices() {
            if c == '(' {
                paren_count += 1;
            } else if c == ')' {
                paren_count -= 1;
                if paren_count == 0 {
                    sig_end_idx = Some(i);
                    break;
                }
            }
        }

        if paren_count != 0 || sig_end_idx.is_none() {
            return Err(Errors::InvalidFunctionSignature);
        }

        let sig_end = sig_end_idx.unwrap();
        let function_signature = call_part[..=sig_end].trim().to_string();
        let mut remaining = call_part[sig_end + 1..].trim();

        let mut value: Option<U256> = None;
        let mut inputs_str = "";

        if !remaining.is_empty() {
            if remaining.starts_with(',') {
                // There is a comma after function signature indicating a value
                remaining = remaining[1..].trim();
                // Check if there's a ':' separating value and inputs
                if let Some(colon_idx) = remaining.find(':') {
                    let value_str = remaining[..colon_idx].trim();
                    value = Some(Self::parse_value(value_str)?);
                    inputs_str = remaining[colon_idx + 1..].trim();
                } else {
                    // No inputs, only value
                    let value_str = remaining.trim();
                    value = Some(Self::parse_value(value_str)?);
                }
            } else if remaining.starts_with(':') {
                // Inputs follow
                inputs_str = remaining[1..].trim();
            } else {
                return Err(Errors::InvalidInput);
            }
        }

        // Parse inputs
        let inputs = if !inputs_str.is_empty() {
            inputs_str
                .split(',')
                .map(|s| s.trim().to_string())
                .collect()
        } else {
            Vec::new()
        };

        Ok((function_signature, value, inputs))
    }

    fn parse_value(value_str: &str) -> Result<U256, Errors> {
        let parts: Vec<&str> = value_str.split_whitespace().collect();
        if parts.len() != 2 {
            return Err(Errors::InvalidInput);
        }
        let amount = U256::from_str(parts[0]).map_err(|_| Errors::InvalidInput)?;
        let multiplier = match parts[1] {
            "wei" => U256::from(1),
            "gwei" => U256::from(1000000000),
            "ether" => U256::from(1000000000000000000 as i64),
            _ => return Err(Errors::InvalidInput),
        };
        Ok(amount * multiplier)
    }

    fn parse_outputs(outputs_str: &str) -> Result<Vec<u8>, Errors> {
        if outputs_str.is_empty() {
            Ok(Vec::new())
        } else {
            let outputs_list: Vec<&str> = outputs_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            let mut outputs = Vec::new();
            for output_arg in outputs_list {
                let output_encoded = Parser::parse_output_arg(output_arg)?;
                outputs.extend_from_slice(output_encoded.as_ref());
            }
            Ok(outputs)
        }
    }
}


