use std::{fs, path::{Path, PathBuf}};
use crate::cmd::semantics::Errors;
use std::collections::HashMap;

const SKIP_DIRECTORY: [&str; 4] = ["externalContracts", "externalSource", "experimental", "multiSource"];
const SKIP_FILE: [&str; 2] = ["access_through_module_name.sol", "multiline_comments.sol"];

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
                continue; // Skip further processing for this directory.
            }
            // Recursively search subdirectories
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

pub(crate) fn extract_functions_from_source(path: &str) -> Result<HashMap<String, Vec<String>>, Errors> {
    let content = fs::read_to_string(path)?;

    let mut contract_functions: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_contract = String::new();
    let mut collecting_functions = false;

    for line in content.lines() {
        if let Some(contract_name) = line.trim().strip_prefix("contract ") {
            // Extract contract name
            let name = contract_name.split_whitespace().next().unwrap_or("");

            current_contract = name.to_string();
            contract_functions.insert(current_contract.clone(), Vec::new());
            collecting_functions = true;
        }

        if collecting_functions && line.contains("function ") {
            if let Some(function_name) = line.split_whitespace().nth(1) {
                let function_signature = function_name.split('(').next().unwrap_or("");
                if let Some(functions) = contract_functions.get_mut(&current_contract) {
                    functions.push(function_signature.to_string());
                }
            }
        }
        else if line.contains(" public ") { 
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

    Ok(contract_functions)
}

