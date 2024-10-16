use std::str::FromStr;

use super::{errors::Errors, utils::count_used_bytes_right};
use alloy_primitives::{keccak256, I256, U256};
use revm::primitives::{Bytes, FixedBytes};

pub struct Parser{}

impl Parser{
    pub(crate) fn parse_function_signature(signature: &str) -> Result<(Vec<u8>, Vec<String>), Errors> {
if let Some(start_idx) = signature.find('(') {
    if let Some(end_idx) = signature.rfind(')') {
        let function_name = &signature[..start_idx];
        let params_str = &signature[start_idx + 1..end_idx];
        let parameter_types = if params_str.is_empty() {
            Vec::new()
        } else {
            params_str
                .split(',')
                .map(|s| s.trim().to_string())
                .collect()
        };

        // Map 'uint' and 'int' to 'uint256' and 'int256' in the parameter types
        let mapped_parameter_types: Vec<String> = parameter_types
            .iter()
            .map(|param| {
                match param.as_str() {
                    "uint" => "uint256".to_string(),
                    "int" => "int256".to_string(),
                    _ => param.clone(),
                }
            })
            .collect();

        // Reconstruct the signature with mapped parameter types
        let new_signature = format!("{}({})", function_name, mapped_parameter_types.join(","));
        // Compute the function selector using the modified signature
        Ok((
            keccak256(new_signature.as_bytes()).0[0..4].to_vec(),
            mapped_parameter_types,
        ))
    } else {
        Err(Errors::InvalidFunctionSignature)
    }
} else {
    Err(Errors::InvalidFunctionSignature)
}}


    pub(crate) fn parse_arg(arg: &str, param_type: &str) -> Result<Bytes, Errors> {
        let arg = arg.trim();

        match param_type {
            "bool" => Self::parse_bool(arg).ok_or(Errors::InvalidArgumentFormat),
            pt if pt.starts_with("uint") => {
                if let Some(bytes) = Self::parse_unsigned_int(arg) {
                    Ok(bytes)
                } else if let Some(bytes) = Self::parse_signed_int(arg) {
                    Ok(bytes)
                } else {
                    Err(Errors::InvalidArgumentFormat)
                }
            }
            pt if pt.starts_with("int") => {
                Self::parse_signed_int(arg).ok_or(Errors::InvalidArgumentFormat)
            }
            pt if pt.starts_with("bytes") => {
                if let Some(bytes) = Self::parse_left(arg)? {
                    Ok(bytes)
                } else if let Some(bytes) = Self::parse_right(arg)? {
                    Ok(bytes)
                } else if let Some(bytes) = Self::parse_raw_hex(arg) {
                    Ok(bytes)
                } else if let Some(bytes) = Self::parse_hex(arg) {
                    Ok(bytes)
                } else {
                    Err(Errors::InvalidArgumentFormat)
                }
            }
            _ => Err(Errors::InvalidArgumentFormat),
        }
    }

    pub(crate) fn parse_output_arg(arg: &str) -> Result<Bytes, Errors> {
        let arg = arg.trim();

        if let Some(bytes) = Self::parse_left(arg)? {
            return Ok(bytes);
        }

        if let Some(bytes) = Self::parse_right(arg)? {
            return Ok(bytes);
        }

        if let Some(bytes) = Self::parse_bool(arg) {
            return Ok(bytes);
        }

        if let Some(bytes) = Self::parse_signed_int(arg) {
            return Ok(bytes);
        }

        if let Some(bytes) = Self::parse_unsigned_int(arg) {
            return Ok(bytes);
        }

        if let Some(bytes) = Self::parse_hex(arg) {
            return Ok(bytes);
        }

        if let Some(bytes) = Self::parse_string(arg) {
            return Ok(bytes);
        }
        
        if let Some(bytes) = Self::parse_raw_hex(arg) {
            return Ok(bytes);
        }
        
        Err(Errors::InvalidArgumentFormat)
    }

    pub(crate) fn parse_bool(arg: &str) -> Option<Bytes> {
        match arg {
            "true" => Some(Bytes::from({
                let mut buf = [0u8; 32];
                buf[31] = 1;
                buf.to_vec()
            })),
            "false" => Some(Bytes::from({
                let mut buf = [0u8; 32];
                buf[31] = 0;
                buf.to_vec()
            })),
            _ => None,
        }
    }

    pub(crate) fn parse_signed_int(arg: &str) -> Option<Bytes> {
        if arg.starts_with('-') {
            I256::from_str(arg).ok().map(|num| {
                let num_bytes = num.to_be_bytes::<32>();
                Bytes::from(num_bytes.to_vec())
            })
        } else {
            None
        }
    }

    pub(crate) fn parse_unsigned_int(arg: &str) -> Option<Bytes> {
        U256::from_str(arg).ok().map(|num| {
            let num_bytes = num.to_be_bytes::<32>();
            Bytes::from(num_bytes.to_vec())
        })
    }

    pub(crate) fn parse_raw_hex(arg: &str) -> Option<Bytes> {
        if arg.starts_with("0x") {
            let hex_str = arg.trim_start_matches("0x");
            let hex_str = if hex_str.len() % 2 != 0 {
                // Odd length, prepend a '0'
                format!("0{}", hex_str)
            } else {
                hex_str.to_string()
            };
            
            hex::decode(&hex_str)
                .ok()
                .as_deref()
                .map(FixedBytes::<32>::left_padding_from)
                .map(Bytes::from)
        } else {
            None
        }
    }
    
    pub(crate) fn parse_hex(arg: &str) -> Option<Bytes> {
        if arg.starts_with("hex\"") {
            let inner = &arg[4..arg.len() - 1];
        match hex::decode(inner) {
            Ok(decoded) => Some(Bytes::from(decoded)),
            Err(e) => {
                println!("Failed to decode hex: {:?}", e);
                None
            }
        }
        } else {
            None
        }
    }

    pub(crate) fn parse_string(arg: &str) -> Option<Bytes> {
        if arg.starts_with('"') && arg.ends_with('"') {
            let inner = &arg[1..arg.len() - 1];
            let string_bytes = inner.as_bytes();
            let output = FixedBytes::<32>::right_padding_from(string_bytes);
            Some(Bytes::from(output.to_vec()))
        } else {
            None
        }
    }

    pub(crate) fn parse_left(arg: &str) -> Result<Option<Bytes>, Errors> {
        if arg.starts_with("left(") && arg.ends_with(')') {
            let inner = &arg[5..arg.len() - 1];
            let inner_bytes = Self::parse_output_arg(inner)?;
            let used_length = count_used_bytes_right(&inner_bytes);
            if used_length == 0 {
                return Ok(Some(Bytes::from(vec![0u8; 32])));
            }
            let first_non_zero = 32 - used_length;
            let used_bytes = &inner_bytes[first_non_zero..];
            let mut output = vec![0u8; 32];
            output[..used_bytes.len()].copy_from_slice(used_bytes);
            Ok(Some(Bytes::from(output)))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn parse_right(arg: &str) -> Result<Option<Bytes>, Errors> {
        if arg.starts_with("right(") && arg.ends_with(')') {
            let inner = &arg[6..arg.len() - 1];
            Self::parse_output_arg(inner).map(Some)
        } else {
            Ok(None)
        }
    }
}
