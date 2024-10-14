use std::{io::Write, path::{Path, PathBuf}, process::{Command, Stdio}};
use std::fs;

use crate::cmd::semantics::Errors;

use alloy_primitives::Bytes;

use super::test_cases::{extract_compile_via_yul, extract_evm_version, parse_calls_and_expectations, TestCase};

const SKIP_DIRECTORY: [&str; 4] = ["externalContracts", "externalSource", "experimental", "multiSource"];
const SKIP_FILE: [&str; 1] = ["access_through_module_name.sol"];

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

pub(crate) fn parse_test_file(path: &str) -> Result<(Bytes, Vec<TestCase>), Errors> {
    let content = fs::read_to_string(path)?;
    let parts: Vec<&str> = content.split("// ----").collect();
    if parts.len() != 2 {
        return Err(Errors::InvalidTestFormat);
    }
    // Early exit if the content contains `==== Source:` We do not handle this yet.
    if content.contains("==== Source:") {
        return Err(Errors::UnhandledTestFormat);  
    }

    let mut is_constructor: bool = false;
    if content.contains("    constructor(") {
        is_constructor = true;  
    }

    let source_code = parts[0];
    let expectations = parts[1].to_string();

    let test_cases = parse_calls_and_expectations(expectations, is_constructor)?;
    let evm_version = extract_evm_version(&content);
    let mut solc_command = Command::new("/usr/local/bin/solc");
    solc_command.arg("--bin").arg("-");

    if let Some(version) = evm_version {
        solc_command.arg("--evm-version").arg(version.to_string());
    }

    if extract_compile_via_yul(&content) {
        solc_command.arg("--via-ir"); 
    }

    let mut solc_process = solc_command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // the below might mean some error are not printed!
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Errors::CompilerNotFound
            } else {
                Errors::CompilationFailed
            }
        })?;

    if let Some(mut stdin) = solc_process.stdin.take() {
        stdin.write_all(source_code.as_bytes()).map_err(|_| Errors::CompilationFailed)?;
    }

    let output = solc_process
        .wait_with_output()
        .map_err(|_| Errors::CompilationFailed)?;

    let stderr_output = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        let has_error = stderr_output
            .lines()
            .any(|line| line.contains("Error")); 

        if has_error {
            eprintln!("Compilation Error: {}", stderr_output); 
            return Err(Errors::CompilationFailed);
        }
    } else {
        if stderr_output.contains("Warning") {
            println!("Compilation Warnings: {}", stderr_output); 
        }
    }
    let binary = output.stdout;

    Ok((Bytes::from(binary), test_cases))
}
