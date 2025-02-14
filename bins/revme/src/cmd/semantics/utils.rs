use log::error;
use revm::db::{CacheDB, EmptyDB};
use revm::primitives::{Address, Bytes, FixedBytes, Log, LogData, U256};

use crate::cmd::semantics::Errors;
use std::collections::HashMap;
use std::{
    fs,
    path::{Path, PathBuf},
};

const SKIP_DIRECTORY: [&str; 5] = [
    "externalContracts",
    "externalSource",
    "experimental",
    "multiSource",
    "isoltestTesting",
];
//in the below, we skip files that are irrelevant for now, that is for example tests for unicode
//escapes or convert_uint_to_fixed_bytes_greater_size
//We also skip test for which we have low understanding: multiple initializations
//We also skip test for difficulty as it gets overwritten by prevrandao
// constructor with param inheritance has a complex structure for us to parse it !
// Block hash not settable, it's fetched via block number!
// codebalance : no need to set-up balances of multiple accounts, we know it works!
// multiple_inheritance : we don't support multiple inheritance
// external_call_at_construction_time: unsure what's being tested
// pass_dynamic_arguments_to_the_base_base_with_gap: multiple inheritance
// same for the below down to transient, for which we need to hardcode further balances to some
// addresses
// virtual functions | array in constructor: nasty inheritance
const SKIP_FILE: [&str; 26] = [
    "access_through_module_name.sol",
    "multiline_comments.sol",
    "unicode_escapes.sol",
    "unicode_string.sol",
    "multiple_initializations.sol",
    "convert_uint_to_fixed_bytes_greater_size.sol",
    "difficulty.sol",
    "constructor_with_params_inheritance_2.sol",
    "blockhash.sol",
    "uncalled_blockhash.sol",
    "blockhash_basic.sol",
    "block_timestamp.sol",
    "tx_gasprice.sol",
    "codebalance_assembly.sol",
    "codehash_assembly.sol",
    "codehash.sol",
    "single_copy_with_multiple_inheritance.sol",
    "external_call_at_construction_time.sol",
    "pass_dynamic_arguments_to_the_base_base_with_gap.sol",
    "pass_dynamic_arguments_to_the_base.sol",
    "pass_dynamic_arguments_to_the_base_base.sol",
    "transient_state_address_variable_members.sol",
    "virtual_functions.sol",
    "base_base_overload.sol",
    "arrays_in_constructors.sol",
    "bytes_in_constructors_packer.sol",
];

pub(crate) fn find_test_files(dir: &Path) -> Result<Vec<PathBuf>, Errors> {
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
                continue;
            }
            test_files.extend(find_test_files(&path)?);
        } else if let Some(extension) = path.extension() {
            if extension == "sol" {
                let should_skip_file = SKIP_FILE.iter().any(|&file| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .map(|name| name == file)
                        .unwrap_or(false)
                });

                if should_skip_file {
                    continue;
                }

                test_files.push(path);
            }
        }
    }
    Ok(test_files)
}

pub(crate) fn extract_compile_via_yul(content: &str) -> bool {
    let parts: Vec<&str> = content.split("// ====").collect();
    if parts.len() < 2 {
        return false;
    }

    for line in parts[1].lines() {
        if let Some(flag_part) = line.trim().strip_prefix("// compileViaYul:") {
            return flag_part.trim() == "true";
        }
    }
    false
}

pub(crate) fn extract_functions_from_source(
    path: &str,
) -> Result<HashMap<String, Vec<String>>, Errors> {
    let content = fs::read_to_string(path)?;

    let mut contract_functions: HashMap<String, Vec<String>> = HashMap::new();
    //parent --> child
    let mut inheritance_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_contract = String::new();
    let mut collecting_functions = false;

    for line in content.lines() {
        if let Some(contract_declaration) = line.trim().strip_prefix("contract ") {
            // Extract contract name and parent (if any)
            let mut parts = contract_declaration.split_whitespace();
            let name = parts.next().unwrap_or("");

            current_contract = name.to_string();
            contract_functions.insert(current_contract.clone(), Vec::new());
            collecting_functions = true;

            // Check for inheritance
            if let Some(is_index) = contract_declaration.find(" is ") {
                let inherited_part = contract_declaration[is_index + 4..].trim();
                let inherited_contracts: Vec<String> = inherited_part
                    .split('{')
                    .map(|s| s.trim().to_string())
                    .collect();
                inheritance_map.insert(
                    current_contract.clone(),
                    vec![inherited_contracts[0].clone()],
                );
            }
        }

        if collecting_functions && line.contains("function ") {
            if let Some(function_name) = line.split_whitespace().nth(1) {
                let function_signature = function_name.split('(').next().unwrap_or("");
                if let Some(functions) = contract_functions.get_mut(&current_contract) {
                    functions.push(function_signature.to_string());
                }
            }
        } else if line.contains("constructor") {
            if let Some(function_name) = line.split_whitespace().next() {
                let function_signature = function_name.split('(').next().unwrap_or("");
                if let Some(functions) = contract_functions.get_mut(&current_contract) {
                    functions.push(function_signature.to_string());
                }
            }
        } else if line.contains("    fallback") || line.contains("    receive") {
            if let Some(functions) = contract_functions.get_mut(&current_contract) {
                functions.push("()".to_string());
            }
        } else if line.contains(" public ") {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if let Some(pos) = tokens.iter().position(|&t| t == "public") {
                if pos + 1 < tokens.len() {
                    let variable_name = tokens[pos + 1].trim_end_matches(';').trim_end_matches('=');
                    if let Some(functions) = contract_functions.get_mut(&current_contract) {
                        functions.push(variable_name.to_string());
                    }
                }
            }
        }
    }

    for (parent, child_contracts) in &inheritance_map {
        for child in child_contracts {
            if let Some(child_functions) = contract_functions.clone().get_mut(child) {
                if let Some(parent_functions) = contract_functions.get_mut(parent) {
                    parent_functions.extend(child_functions.clone());
                }
            }
        }
    }
    Ok(contract_functions)
}

pub(crate) fn count_used_bytes_right(bytes: &[u8]) -> usize {
    let mut start = 0;
    while start < bytes.len() && bytes[start] == 0 {
        start += 1;
    }
    bytes.len() - start
}

pub(crate) fn parse_string_with_escapes(s: &str) -> Result<Vec<u8>, Errors> {
    let mut bytes = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('x') => {
                    // Hex escape sequence \xNN
                    let hex_digits: String = chars.by_ref().take(2).collect();
                    if hex_digits.len() != 2 {
                        return Err(Errors::InvalidInput);
                    }
                    let byte =
                        u8::from_str_radix(&hex_digits, 16).map_err(|_| Errors::InvalidInput)?;
                    bytes.push(byte);
                }
                Some('u') => {
                    // Unicode escape sequence \uXXXX or \u{XXXXXX}
                    let mut hex = String::new();
                    if chars.peek() == Some(&'{') {
                        chars.next(); // Consume '{'
                        while let Some(&next_char) = chars.peek() {
                            if next_char == '}' {
                                chars.next(); // Consume '}'
                                break;
                            } else {
                                hex.push(chars.next().unwrap());
                            }
                        }
                    } else {
                        // Expect 4 hex digits
                        hex = chars.by_ref().take(4).collect();
                    }
                    let codepoint =
                        u32::from_str_radix(&hex, 16).map_err(|_| Errors::InvalidInput)?;
                    let s = std::char::from_u32(codepoint).ok_or(Errors::InvalidInput)?;
                    let mut buf = [0; 4];
                    let encoded = s.encode_utf8(&mut buf);
                    bytes.extend_from_slice(encoded.as_bytes());
                }
                Some('n') => bytes.push(b'\n'),
                Some('r') => bytes.push(b'\r'),
                Some('t') => bytes.push(b'\t'),
                Some('0') => bytes.push(b'\0'),
                Some('\'') => bytes.push(b'\''),
                Some('"') => bytes.push(b'"'),
                Some('\\') => bytes.push(b'\\'),
                Some(_) => {
                    return Err(Errors::InvalidInput);
                }
                None => {
                    return Err(Errors::InvalidInput);
                }
            }
        } else {
            // Regular character
            bytes.extend(c.to_string().as_bytes());
        }
    }
    Ok(bytes)
}

/// Verifies that the emitted logs (ignoring address and topics) match the expected events.
pub(crate) fn verify_emitted_events(
    expected_events: &[LogData],
    emitted_logs: &[Log],
) -> Result<(), Errors> {
    if expected_events.len() != emitted_logs.len() {
        error!(
            "Expected {} events, but {} were emitted",
            expected_events.len(),
            emitted_logs.len()
        );
        return Err(Errors::LogMismatch);
    }
    for (expected, log) in expected_events.iter().zip(emitted_logs.iter()) {
        if expected.topics() != log.topics() {
            error!(
                "Mismatch in event topics. Expected: {:?}, Got: {:?}",
                expected.topics(),
                log.topics()
            );
            return Err(Errors::LogMismatch);
        }
        if expected.data != log.data.data {
            error!(
                "Mismatch in event data. Expected: {:?}, Got: {:?}",
                expected.data, log.data.data
            );
            return Err(Errors::LogMismatch);
        }
    }
    Ok(())
}

pub(crate) fn verify_expected_balances(
    mut db: CacheDB<EmptyDB>,
    expected: &HashMap<Address, U256>,
    deployed_contract_address: Address,
) -> Result<(), Errors> {
    for (addr, exp_balance) in expected {
        let account = db
            .load_account(if addr == &Address::ZERO {
                deployed_contract_address
            } else {
                *addr
            })
            .unwrap();

        if account.info.balance != *exp_balance {
            error!(
                "Balance mismatch for {}: expected {}, got {}",
                addr, exp_balance, account.info.balance
            );
            return Err(Errors::BalanceMismatch);
        }
    }
    Ok(())
}

// Helper function to convert Bytes into FixedBytes<32>
pub(crate) fn bytes_to_fixed(bytes: Bytes) -> FixedBytes<32> {
    let slice = bytes.as_ref();
    let mut fixed = [0u8; 32];
    fixed.copy_from_slice(slice);
    fixed.into()
}
