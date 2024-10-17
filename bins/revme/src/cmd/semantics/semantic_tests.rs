use std::{io::Write, process::{Command, Stdio}};
use std::fs;

use revm::primitives::Bytes;

use crate::cmd::semantics::Errors;

use super::{compiler_evm_versions::EVMVersion, test_cases::TestCase, utils::{extract_compile_via_yul, extract_functions_from_source}};

const SKIP_KEYWORD: [&str; 1] = ["library "];

#[derive(Debug, Clone)]
pub struct ContractInfo {
    pub contract_name: String,
    pub compile_binary: Bytes, 
    pub functions: Vec<String>,        
}

impl ContractInfo {
    pub fn new(contract_name: String, compile_binary: Bytes) -> Self {
        Self {
            contract_name,
            compile_binary,
            functions: Vec::new(), 
        }
    }

    pub fn add_function(&mut self, function_name: String) {
        self.functions.push(function_name);
    }

    pub fn has_function(&self, function_name: &str) -> bool {
        self.functions.iter().any(|f| f == function_name)
    }

    pub fn has_fallback_function(&self) -> bool {
        self.functions.iter().any(|f| f == "()")
    }
}

#[derive(Debug, Clone)]
pub struct SemanticTests {
    pub test_cases: Vec<TestCase>,
    pub contract_infos: Vec<ContractInfo>,
}

impl SemanticTests {
    pub fn new(path: &str) -> Result<Self, Errors> {
        let content = fs::read_to_string(path)?;
        let parts: Vec<&str> = content.split("// ----").collect();
        if parts.len() != 2 {
            return Err(Errors::InvalidTestFormat);
        }
        
        // Early exit if the content contains `==== Source:` We do not handle this yet nor
        // nonExistingFunctions nor Libraries that generate some slightly different Bytecode with
        // the unhandled "_"
        if content.contains("==== Source:") || content.contains("allowNonExistingFunctions: true") || content.contains("// library:"){
            return Err(Errors::UnhandledTestFormat);  
        }

        let expectations = parts[1].to_string();

        let evm_version = EVMVersion::extract(&content);
        let via_ir = extract_compile_via_yul(&content);

        let contract_infos = Self::get_contract_infos(path, evm_version.clone(), via_ir, false)?;

        let test_cases = TestCase::from_expectations(expectations, &contract_infos)?;
        Ok(SemanticTests {
            test_cases,
            contract_infos,
        })
    }
    
    fn compile_solidity(
        path: &str, 
        evm_version: Option<EVMVersion>, 
        via_ir: bool, 
        runtime: bool
    ) -> Result<String, Errors> { 
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

       Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn get_contract_infos(
        path: &str, 
        evm_version: Option<EVMVersion>, 
        via_ir: bool, 
        runtime: bool
    ) -> Result<Vec<ContractInfo>, Errors> {  
        let stdout_output = Self::compile_solidity(path, evm_version, via_ir, runtime)?;

        let mut contract_infos = Vec::new();

        let contract_sections = stdout_output.split("======= ").skip(1);

        for section in contract_sections {
            let mut lines = section.lines();
            let contract_line = lines.next().unwrap_or("");

            let contract_name = contract_line
                .split(':')
                .nth(1)
                .unwrap_or("Unknown")
                .trim_end_matches(" =======")
                .to_string();

            let rest_of_section = lines.collect::<Vec<&str>>().join("\n");
            if let Some(index) = rest_of_section.find("Binary:") {
                let after_binary = &rest_of_section[index + "Binary:".len()..];
                let bytecode_line = after_binary.lines().skip(1).next().unwrap_or("");
                let compile_binary = match hex::decode(bytecode_line.trim()) {
                    Ok(decoded_bytes) => Bytes::from(decoded_bytes),
                    Err(decode_error) => {
                        eprintln!("Failed to decode bytecode line: {}, error: {:?}", bytecode_line.trim(), decode_error);

                        return Err(Errors::CompilationFailed);
                    }
                };
                let mut contract_info = ContractInfo::new(contract_name.clone(), compile_binary);

                let contract_functions_map = extract_functions_from_source(path)?;
                if let Some(functions) = contract_functions_map.get(&contract_name) {
                    for function in functions {
                        contract_info.add_function(function.clone());
                    }
                }

                contract_infos.push(contract_info);
            }
        }
        //reversing as in the more down a function is seen the more likely it is the one we want to
        //call
        contract_infos.reverse();
        Ok(contract_infos)
    }
}

