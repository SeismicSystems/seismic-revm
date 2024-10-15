use std::{fs, path::{Path, PathBuf}};
use crate::cmd::semantics::Errors;

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

pub(crate) fn extract_functions_from_source(path: &str, contract_name: &str) -> Result<Vec<String>, Errors> {
    let content = fs::read_to_string(path)?;

    let mut functions = Vec::new();
    let mut inside_contract = false;

    for line in content.lines() {
        if line.contains(&format!("contract {}", contract_name)) {
            inside_contract = true;
        } else if inside_contract && line.contains("function ") {
            if let Some(function_name) = line.split_whitespace().nth(1) {
                let function_signature = function_name.split('(').next().unwrap_or("");
                functions.push(function_signature.to_string());
            }
        } else if inside_contract && line.contains("}") { 
            break;
        }
    }

    Ok(functions)
}

