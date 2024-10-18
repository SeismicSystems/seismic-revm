use crate::cmd::semantics::Errors;
use std::collections::HashMap;
use std::{
    fs,
    path::{Path, PathBuf},
};

const SKIP_DIRECTORY: [&str; 4] = [
    "externalContracts",
    "externalSource",
    "experimental",
    "multiSource",
];
//in the below, we skip files that are irrelevant for now, that is for example tests for unicode
//escapes or convert_uint_to_fixed_bytes_greater_size
//We also skip test for which we have low understanding: multiple initializations
//We also skip test for difficulty as it gets overwritten by prevrandao
// constructor with param inheritance has a complex structure for us to parse it !
// Block hash not settable, it's fetched via block number!
const SKIP_FILE: [&str; 13] = [
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
    "tx_gasprice.sol"
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
            if let Some(function_name) = line.split_whitespace().nth(0) {
                let function_signature = function_name.split('(').next().unwrap_or("");
                if let Some(functions) = contract_functions.get_mut(&current_contract) {
                    functions.push(function_signature.to_string());
                }
            }
        } else if line.contains("    fallback") || line.contains("    receive ()") {
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
