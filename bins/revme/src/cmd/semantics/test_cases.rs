use std::str::FromStr;

use super::{errors::Errors, parser::Parser, semantic_tests::ContractInfo, utils::bytes_to_fixed};
use log::{debug, info};
use revm::primitives::{keccak256, Address, Bytes, FixedBytes, HashMap, LogData, U256};

const SKIP_KEYWORD: [&str; 1] = ["gas"];

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ExecutionResult {
    Success,
    Failure,
}

impl Default for ExecutionResult {
    fn default() -> Self {
        Self::Success
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ExpectedOutputs {
    state: ExecutionResult,
    pub output: Bytes,
}

impl ExpectedOutputs {
    pub(crate) fn is_success(&self) -> bool {
        self.state == ExecutionResult::Success
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TestCase {
    pub steps: Vec<TestStep>,
}

#[derive(Debug, Clone)]
pub(crate) enum TestStep {
    Deploy { contract: Bytes, value: U256 },
    CallFunction { function_name: String, input_data: Bytes, expected_outputs: ExpectedOutputs, value: U256 },
    CheckStorageEmpty { expected_empty: bool },
    CheckBalance { expected_balances: HashMap<Address, U256> },
    CheckEvents { expected_events: Vec<LogData> },
}

impl TestCase {
    pub(crate) fn from_expectations(
        expectations: String,
        contract_infos: &[ContractInfo],
    ) -> Result<Vec<Self>, Errors> {
        let mut test_cases = Vec::new();
        let mut steps = Vec::new();

        let mut first_contract_deployed = false;

        for line in expectations.lines().map(str::trim).filter(|l| !l.is_empty()) {
            debug!("Parsing line: {}", line);
            let line = Self::strip_comments(line);

            if line.contains("~ emit") {
                let event_bytes = Self::parse_event(&line);
                steps.push(TestStep::CheckEvents { expected_events: vec![event_bytes] });
                continue;
            }

            if line.starts_with("balance") {
                 if line.contains("balance:") || line.starts_with("balance ->") {
                    let (address, balance) = Self::parse_balance(&line)?; 
                    steps.push(TestStep::CheckBalance {
                        expected_balances: vec![(address, balance)].into_iter().collect(),
                    });
                    continue;
                }
            }

            if line.starts_with("storageEmpty") {
                if line.contains("->") {
                    let storage_empty = Self::parse_storage_empty(&line)?;
                    steps.push(TestStep::CheckStorageEmpty { expected_empty: storage_empty });
                }
                continue;
            }

            if !steps.is_empty() {
                test_cases.push(TestCase { steps: steps.clone() });
                steps.clear();
            }

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

            let (function_signature, value, inputs) = Self::parse_call_part(&call_part)?;
            let expected_outputs = Self::parse_outputs(expected_output_part)?;
            let (function_selector, _) = Parser::parse_function_signature(&function_signature)?;
            let is_constructor = function_signature.starts_with("constructor(");

            let mut args_encoded = Vec::new();
            for arg_str in &inputs {
                args_encoded.push(Parser::parse_arg(arg_str)?);
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
                    contract.has_function(function_signature.split('(').next().unwrap_or("").trim())
                }
            });

            if let Some(contract) = matching_contract {
                let deploy_binary = contract.compile_binary.clone();
                if is_constructor {
                    for arg in &args_encoded {
                        deploy_binary.to_vec().extend_from_slice(arg);
                    }
                    input_data.clear();
                }

                if is_constructor {
                    steps.push(TestStep::Deploy { contract: deploy_binary.into(), value: value.unwrap_or_default() });
                    first_contract_deployed = true;
                    continue;
                } else if !first_contract_deployed {
                    steps.insert(0, TestStep::Deploy { contract: deploy_binary.into(), value: value.unwrap_or_default() });
                    first_contract_deployed = true;
                }
                steps.push(TestStep::CallFunction {
                function_name: function_signature.clone(),
                input_data: input_data.into(),
                expected_outputs,
                value: value.unwrap_or_default()
                });
            } else {
                info!("No matching contract found for function: {}", function_signature);
            }
        }

        if !steps.is_empty() {
            test_cases.push(TestCase { steps });
        }

        Ok(test_cases)
    }

    fn parse_event(call_part: &str) -> LogData {
        // Remove the "~ emit" prefix and trim whitespace.
        let event_str = call_part.trim().trim_start_matches("~ emit").trim();
        // Split at the first colon to separate signature and arguments.
        let parts: Vec<&str> = event_str.splitn(2, ':').collect();

        // Process the event signature.
        // Remove any trailing " from <address>" part if present.
        let mut signature = parts[0].trim();
        if let Some(pos) = signature.find(" from ") {
            signature = signature[..pos].trim();
        }

        let mut topics: Vec<FixedBytes<32>> = Vec::new();
        let mut data = Vec::new();

        // For non-anonymous events, compute and push the keccak256 hash of the signature as the first topic.
        if signature != "<anonymous>" {
            let function_signature = keccak256(signature.as_bytes());
            topics.push(function_signature);
        }

        // Process event arguments if present after the colon.
        if parts.len() == 2 {
            let args_str = parts[1].trim();
            if !args_str.is_empty() {
                for arg in args_str.split(',') {
                    let arg = arg.trim();
                    if arg.is_empty() {
                        continue;
                    }
                    if arg.starts_with('#') {
                        // Indexed parameter: remove '#' and parse hex.
                        let hex_str = arg.trim_start_matches('#').trim();
                        let parsed = Parser::parse_arg(hex_str).unwrap();
                        topics.push(bytes_to_fixed(parsed));
                    } else {
                        // Non-indexed parameter: parse hex and append to data.
                        let parsed = Parser::parse_arg(arg).unwrap();
                        data.extend(parsed);
                    }
                }
            }
        }

        LogData::new(topics, data.into()).unwrap()
    }

    fn parse_balance(line: &str) -> Result<(Address, U256), Errors> {
        let trimmed = line.trim();

        // Remove the "balance:" prefix and trim whitespace.
        let balance_line = if trimmed.starts_with("balance:") {
            trimmed.trim_start_matches("balance:").trim()
        } else if trimmed.starts_with("balance") {
            trimmed.trim_start_matches("balance").trim()
        } else {
            return Err(Errors::InvalidInput);
        };

        // Expected formats:
        // 1. "0xADDRESS -> VALUE"
        // 2. "-> VALUE"  (no address provided; use default)
        let parts: Vec<&str> = balance_line.split("->").collect();
        if parts.len() != 2 {
            return Err(Errors::InvalidInput);
        }
        let address_str = parts[0].trim();
        let balance_str = parts[1].trim();

        // Use the provided address if available; otherwise, use the default address. We'll use
        // this to understand we should use the deployed contract address downstream.
        let address = if address_str.is_empty() {
            Address::ZERO
        } else {
            Address::from_str(address_str).map_err(|_| Errors::InvalidInput)?
        };

        // Parse the balance (in decimal or hex, as needed).
        let balance = U256::from_str(balance_str).map_err(|_| Errors::InvalidInput)?;

        Ok((address, balance))
    }

    fn parse_storage_empty(line: &str) -> Result<bool, Errors> {
        // Remove "storageEmpty" keyword.
        let trimmed = line.trim().trim_start_matches("storageEmpty").trim();
        // Expect the format "-> VALUE"
        if !trimmed.starts_with("->") {
            return Err(Errors::InvalidInput);
        }
        let value_str = trimmed.trim_start_matches("->").trim();
        match value_str {
            "0" => Ok(false),
            "1" => Ok(true),
            _ => Err(Errors::InvalidInput)
        }
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
            } else if let Some(stripped) = remaining.strip_prefix(':') {
                // Inputs follow
                inputs_str = stripped.trim();
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
            "ether" => U256::from(1000000000000000000_i64),
            _ => return Err(Errors::InvalidInput),
        };
        Ok(amount * multiplier)
    }

    fn parse_outputs(outputs_str: &str) -> Result<ExpectedOutputs, Errors> {
        if outputs_str.is_empty() {
            Ok(ExpectedOutputs::default())
        } else if outputs_str.contains("FAILURE") {
            let outputs_list: Vec<&str> = outputs_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && !s.contains("FAILURE"))
                .collect();

            let mut output = vec![];
            for output_arg in outputs_list {
                let output_encoded = Parser::parse_arg(output_arg)?;
                output.extend_from_slice(output_encoded.as_ref());
            }

            Ok(ExpectedOutputs {
                state: ExecutionResult::Failure,
                output: output.into(),
            })
        } else {
            let outputs_list: Vec<&str> = outputs_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            let mut output = Vec::new();
            for output_arg in outputs_list {
                let output_encoded = Parser::parse_arg(output_arg)?;
                output.extend_from_slice(output_encoded.as_ref());
            }
            Ok(ExpectedOutputs {
                state: ExecutionResult::Success,
                output: output.into(),
            })
        }
    }

    fn strip_comments(line: &str) -> String {
        let line = if line.starts_with("//") {
            line.trim_start_matches("//").trim()
        } else {
            line
        };
        if !line.contains("~ emit") {
            if let Some(comment_idx) = line.find('#') {
                return line[..comment_idx].trim().to_string();
            }
        }
        line.to_string()
    }
}
