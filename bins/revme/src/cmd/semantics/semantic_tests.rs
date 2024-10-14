use std::{io::Write, process::{Command, Stdio}};
use std::fs;

use revm::primitives::Bytes;

use crate::cmd::semantics::Errors;

use super::{compiler_evm_versions::EVMVersion, test_cases::TestCase, utils::extract_compile_via_yul};

#[derive(Debug)]
pub struct SemanticTests {
    pub test_cases: Vec<TestCase>,
    pub source_code: Option<Bytes>,
    pub runtime_code: Bytes
}

impl SemanticTests {
    pub fn new(path: &str) -> Result<Self, Errors> {
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

        let test_cases = TestCase::from_expectations(expectations)?;
        let evm_version = EVMVersion::extract(&content);
        let via_ir = extract_compile_via_yul(&content);

        let runtime_code = Self::compile_solidity(path, evm_version.clone(), via_ir, true)?;

        let binary = if test_cases.iter().any(|tc| tc.is_constructor) {
            Some(Self::compile_solidity(source_code, evm_version, via_ir, false)?)
        } else {
            None 
        };

        Ok(SemanticTests {
            test_cases,
            source_code: binary,
            runtime_code,
        })
    }

    fn compile_solidity(
        path: &str, 
        evm_version: Option<EVMVersion>, 
        via_ir: bool, 
        runtime: bool
    ) -> Result<Bytes, Errors> {
        let mut solc_command = Command::new("/usr/local/bin/solc");

        if runtime {
            solc_command.arg("--bin-runtime");
        } else {
            solc_command.arg("--bin");
        }
        solc_command.arg(path);

        if let Some(version) = evm_version {
            solc_command.arg("--evm-version").arg(version.to_string());
        }

        if via_ir {
            solc_command.arg("--via-ir"); 
        }

        let output = solc_command
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Errors::CompilerNotFound
                } else {
                    Errors::CompilationFailed
                }
            })?;

        if !output.status.success() {
            return Err(Errors::CompilationFailed);
        }
    
        let stdout_output = String::from_utf8_lossy(&output.stdout);

        if let Some(bytecode_pos) = stdout_output.find("Binary of the runtime part:") {
            let bytecode = stdout_output[bytecode_pos..]
                .lines() 
                .skip(1) 
                .next()  
                .unwrap_or(""); 

            Ok(Bytes::from(hex::decode(bytecode.trim()).map_err(|_| Errors::CompilationFailed)?))
        } else {
            Err(Errors::CompilationFailed)
        }

    }
}

