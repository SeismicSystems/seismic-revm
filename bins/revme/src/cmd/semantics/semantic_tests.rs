use std::fs;
use std::process::Command;

use log::error;
use regex::Regex;
use revm::primitives::{hex, Address, Bytes};

use super::{
    compiler_evm_versions::EVMVersion,
    test_cases::TestCase,
    utils::{extract_compile_via_yul, extract_functions_from_source, needs_eof},
    Errors,
};

const SKIP_KEYWORD: [&str; 3] = [
    "==== Source:",
    "allowNonExistingFunctions: true",
    "revertStrings: debug",
];

#[derive(Debug, Clone)]
pub struct ContractInfo {
    pub contract_name: String,
    pub evm_version: EVMVersion,
    compile_binary: String,
    pub functions: Vec<String>,
    pub is_library: bool,
    deploy_args: Vec<u8>,
}

impl ContractInfo {
    // Updated new function to default to SpecId::LATEST
    pub fn new(
        contract_name: String,
        compile_binary: String,
        evm_version: EVMVersion,
        is_library: bool,
    ) -> Self {
        Self {
            contract_name,
            evm_version,
            compile_binary,
            functions: Vec::new(),
            is_library,
            deploy_args: vec![],
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

    pub fn add_deploy_args(&mut self, args: Vec<Bytes>) {
        let mut buffer: Vec<u8> = Vec::new();
        for arg in args {
            buffer.extend_from_slice(arg.as_ref());
        }
        self.deploy_args = buffer;
    }

    pub fn set_is_library(&mut self, is_library: bool) {
        self.is_library = is_library;
    }

    pub fn get_deployable_code(&self, address: Option<Address>) -> Bytes {
        let mut code_str = self.compile_binary.clone();

        if let Some(addr) = address {
            let mut addr_str = addr.to_string(); // e.g. "0x123abc..."

            if addr_str.starts_with("0x") {
                addr_str = addr_str.trim_start_matches("0x").to_string();
            }

            let re = Regex::new(r"__\$[0-9a-fA-F]+\$__").expect("invalid regex");
            code_str = re.replace_all(&code_str, &addr_str).to_string();
        }

        let mut code_bytes = match hex::decode(&code_str) {
            Ok(bytes) => bytes,
            Err(decode_error) => {
                error!(
                    "Failed to decode bytecode string: {}, error: {:?}",
                    code_str, decode_error
                );
                return Bytes::new();
            }
        };

        code_bytes.extend_from_slice(&self.deploy_args);

        Bytes::from(code_bytes)
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
        if SKIP_KEYWORD
            .iter()
            .any(|&keyword| content.contains(keyword))
        {
            return Err(Errors::UnhandledTestFormat);
        }
        let expectations = parts[1].to_string();

        let evm_version = EVMVersion::extract(&content);
        let via_ir = extract_compile_via_yul(&content);
        let eof_mode = needs_eof(&content);

        let mut contract_infos =
            Self::get_contract_infos(path, evm_version, via_ir, eof_mode, false)?;

        let test_cases = TestCase::from_expectations(expectations, &mut contract_infos[..])?;
        Ok(SemanticTests {
            test_cases,
            contract_infos,
        })
    }

    fn compile_solidity(
        path: &str,
        evm_version: Option<EVMVersion>,
        via_ir: bool,
        eof_mode: bool,
        runtime: bool,
    ) -> Result<String, Errors> {
        let mut solc = Command::new("/usr/local/bin/solc");

        solc.arg(if runtime { "--bin-runtime" } else { "--bin" })
            .arg(path);

        if let Some(v) = evm_version {
            solc.arg("--evm-version").arg(v.to_string());
        }

        // via‑IR is required for EOF; keep explicit flag for legacy tests
        if via_ir || eof_mode {
            solc.arg("--via-ir");
        }

        if eof_mode {
            solc.arg("--experimental-eof-version").arg("1");
        }
        // ─── invoke ───────────────────────────────────────────────────────────────
        let output = solc.output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Errors::CompilerNotFound
            } else {
                error!("Compilation failed for file: {:?}", path);
                Errors::CompilationFailed
            }
        })?;

        if !output.status.success() {
            error!(
                "Compilation failed for file: {:?}, output: {:?}",
                path, output
            );
            return Err(Errors::CompilationFailed);
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn get_contract_infos(
        path: &str,
        evm_version: Option<EVMVersion>,
        via_ir: bool,
        eof_mode: bool,
        runtime: bool,
    ) -> Result<Vec<ContractInfo>, Errors> {
        let stdout_output = Self::compile_solidity(path, evm_version, via_ir, eof_mode, runtime)?;

        let revm_version = evm_version.unwrap_or(EVMVersion::Mercury);

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
                let bytecode_line = after_binary.lines().nth(1).unwrap_or("");

                let mut contract_info = ContractInfo::new(
                    contract_name.clone(),
                    bytecode_line.to_string(),
                    revm_version,
                    false,
                );

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
