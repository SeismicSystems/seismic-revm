use std::{io::Write, path::{Path, PathBuf}, process::{Command, Stdio}};
use std::fs;

use crate::cmd::semantics::Errors;

use alloy_primitives::Bytes;

use super::{compiler_evm_versions::EVMVersion, test_cases::{extract_compile_via_yul, extract_evm_version, parse_calls_and_expectations, TestCase}};

const SKIP_DIRECTORY: [&str; 4] = ["externalContracts", "externalSource", "experimental", "multiSource"];
const SKIP_FILE: [&str; 1] = ["access_through_module_name.sol"];

pub struct FullTest {
    pub test_cases: Vec<TestCase>,
    pub source_code: Option<Bytes>,
    pub runtime_code: Bytes
}

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

pub(crate) fn parse_test_file(path: &str) -> Result<FullTest, Errors> {
    let content = fs::read_to_string(path)?;
    let parts: Vec<&str> = content.split("// ----").collect();
    if parts.len() != 2 {
        return Err(Errors::InvalidTestFormat);
    }
    // Early exit if the content contains `==== Source:` We do not handle this yet.
    if content.contains("==== Source:") {
        return Err(Errors::UnhandledTestFormat);  
    }

    let source_code = parts[0];
    let expectations = parts[1].to_string();

    let test_cases = parse_calls_and_expectations(expectations)?;
    let evm_version = extract_evm_version(&content);
    let via_ir = extract_compile_via_yul(&content);

    let runtime_code = compile_solidity(source_code, evm_version.clone(), via_ir, true)?;

    let binary = if test_cases.iter().any(|tc| tc.is_constructor) {
        Some(compile_solidity(source_code, evm_version, via_ir, false)?)
    } else {
        None 
    };

    // Return FullTest
    Ok(FullTest {
        test_cases,
        source_code: binary,
        runtime_code,
    })
}

pub(crate) fn compile_solidity(
    source_code: &str, 
    evm_version: Option<EVMVersion>, 
    via_ir: bool, 
    runtime: bool
) -> Result<Bytes, Errors> {
    let mut solc_command = Command::new("/usr/local/bin/solc");

    // Add either --bin or --bin-runtime based on the flag
    if runtime {
        solc_command.arg("--bin-runtime");
    } else {
        solc_command.arg("--bin");
    }

    solc_command.arg("-");

    if let Some(version) = evm_version {
        solc_command.arg("--evm-version").arg(version.to_string());
    }

    if via_ir {
        solc_command.arg("--via-ir"); 
    }

    // Spawn the solc process
    let mut solc_process = solc_command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
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

    if !output.status.success() {
        return Err(Errors::CompilationFailed);
    }

    Ok(Bytes::from(output.stdout)) // Return the compiled binary as Bytes
}

