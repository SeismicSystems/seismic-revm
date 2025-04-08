//! Precompiles Added for the Seismic Chain
//!
//! This module provides several precompiles enabled by the Seismic Chain
//! features. Below is an overview of the available precompiles and their
//! functionalities:
//!
//! # Modules
//!
//! - [`aes`]: Provides AES-GCM encryption and decryption functionalities.
//! - [`ecdh_derive_sym_key`]: Implements symmetric key derivation using
//!     Elliptic Curve Diffie-Hellman (ECDH) with the secp256k1 curve,
//!     producing an AES-compatible key.
//! - [`hkdf_derive_sym_key`]: Implements key derivation using the HKDF
//!     (HMAC-based Key Derivation Function) algorithm to derive AES-compatible
//!     symmetric keys from raw input bytes.
//! - [`rng`]: Generates cryptographically secure random bytes. The randomness
//!     is based on a secret Verifiable Random Function (VRF) key and the
//!     block's transcript.

pub mod aes;
pub mod ecdh_derive_sym_key;
pub mod hkdf_derive_sym_key;
pub mod secp256k1_sign;
pub mod rng;
pub mod stateful_precompile;
pub use stateful_precompile::StatefulPrecompiles;

use crate::{api::exec::SeismicContextTr, SeismicSpecId};
use once_cell::race::OnceBox;
use revm::{
    context::Cfg,
    handler::{EthPrecompiles, PrecompileProvider},
    interpreter::{Gas, InstructionResult, InterpreterResult},
    precompile::{
        secp256r1, PrecompileError, Precompiles
    },
    primitives::{Address, Bytes},
};
use std::boxed::Box;
use std::string::String;

#[derive(Debug, Clone)]
pub struct SeismicPrecompiles<CTX: SeismicContextTr> {
    inner: EthPrecompiles,
    stateful_precompiles: StatefulPrecompiles<CTX>,
}

impl <CTX: SeismicContextTr> SeismicPrecompiles<CTX> {
    /// Create a new [`SeismicPrecompiles`] with the given precompiles.
    pub fn new(precompiles: (&'static Precompiles, StatefulPrecompiles<CTX>)) -> Self {
        Self {
            inner: EthPrecompiles { precompiles: precompiles.0 },
            stateful_precompiles: precompiles.1,
        }
    }

    /// Create a new precompile provider with the given optimismispec.
    #[inline]
    pub fn new_with_spec(spec: SeismicSpecId) -> Self {
        match spec {
            _spec @  SeismicSpecId::MERCURY => Self::new(mercury::<CTX>()),
        }
    }
}

/// Returns precompiles for MERCURY spec.
pub fn mercury<CTX: SeismicContextTr>() -> (&'static Precompiles, StatefulPrecompiles<CTX>) {
    // Store only the stateless precompiles in the static OnceBox
    static INSTANCE: OnceBox<Precompiles> = OnceBox::new();
    
    let regular_precompiles = INSTANCE.get_or_init(|| {
        let mut precompiles = Precompiles::prague().clone();
        precompiles.extend([
            secp256r1::P256VERIFY,
            ecdh_derive_sym_key::ECDH,
            hkdf_derive_sym_key::HKDF,
            aes::aes_gcm_enc::AES_GCM_ENC,
            aes::aes_gcm_dec::AES_GCM_DEC,
            secp256k1_sign::SECP256K1_SIGN,
        ]);
        Box::new(precompiles)
    });
    
    //TODO: check how expensive is the below instead of a single init! issue with generics
    let mut stateful_precompiles = StatefulPrecompiles::new();
    stateful_precompiles.extend(rng::precompile::rng_precompile_iter::<CTX>().map(|p| (p.0, p.1)));
    (regular_precompiles, stateful_precompiles)
}

impl<CTX> PrecompileProvider<CTX> for SeismicPrecompiles<CTX>
where
    CTX: SeismicContextTr,
    CTX::Cfg: Cfg<Spec = SeismicSpecId>,
{
    type Output = InterpreterResult;
    
    #[inline]
    fn set_spec(&mut self, spec: <CTX::Cfg as Cfg>::Spec) {
        *self = Self::new_with_spec(spec);
    }
    
    #[inline]
    fn run(
        &mut self,
        context: &mut CTX,
        address: &Address,
        bytes: &Bytes,
        gas_limit: u64,
    ) -> Result<Option<Self::Output>, String> {
        if let Some(precompile) = self.stateful_precompiles.get(address) {
            let mut result = InterpreterResult {
                result: InstructionResult::Return,
                gas: Gas::new(gas_limit),
                output: Bytes::new(),
            };
            
            match (*precompile)(context, bytes, gas_limit) {
                Ok(output) => {
                    let underflow = result.gas.record_cost(output.gas_used);
                    assert!(underflow, "Gas underflow is not possible");
                    result.result = InstructionResult::Return;
                    result.output = output.bytes;
                }
                Err(PrecompileError::Fatal(e)) => return Err(e),
                Err(e) => {
                    result.result = if e.is_oog() {
                        InstructionResult::PrecompileOOG
                    } else {
                        InstructionResult::PrecompileError
                    };
                }
            }
            
            Ok(Some(result))
        } else {
            // Fall back to standard precompiles
            self.inner.run(context, address, bytes, gas_limit)
        }
    }
    
    #[inline]
    fn warm_addresses(&self) -> Box<impl Iterator<Item = Address>> {
        // Combine both standard and stateful precompile addresses
        let standard_addresses = self.inner.warm_addresses().into_iter();
        let stateful_addresses = self.stateful_precompiles.addresses().cloned();
        
        Box::new(standard_addresses.chain(stateful_addresses))
    }
    
    #[inline]
    fn contains(&self, address: &Address) -> bool {
        self.inner.contains(address) || self.stateful_precompiles.contains(address)
    }
}

impl<CTX: SeismicContextTr> Default for SeismicPrecompiles<CTX>
where
    CTX::Cfg: Cfg, 
{
    fn default() -> Self {
        Self::new_with_spec(SeismicSpecId::MERCURY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use revm::{database::EmptyDB, primitives::hex};
    
    use crate::{SeismicContext, DefaultSeismic};

    #[test]
    fn test_cancun_precompiles_in_mercury() {
        assert_eq!(mercury::<SeismicContext<EmptyDB>>().0.difference(Precompiles::prague()).len(), 6)
    }

    #[test]
    fn test_default_precompiles_is_latest() {
        let latest = SeismicPrecompiles::<SeismicContext<EmptyDB>>::new_with_spec(SeismicSpecId::default())
            .inner
            .precompiles;
        let default = SeismicPrecompiles::<SeismicContext<EmptyDB>>::default().inner.precompiles;
        assert_eq!(latest.len(), default.len());

        let intersection = default.intersection(latest);
        assert_eq!(intersection.len(), latest.len())
    }

    #[test]
    fn test_seismic_precompiles_rng() {
        let mut precompiles = SeismicPrecompiles::<SeismicContext<EmptyDB>>::new_with_spec(SeismicSpecId::MERCURY);
        let mut context = SeismicContext::<EmptyDB>::seismic();
        let rng_address = *precompiles.stateful_precompiles.addresses().next().expect("RNG precompile address should exist");
        
        let bytes_requested: u32 = 32;
        let personalization = vec![0xAA, 0xBB, 0xCC, 0xDD];
        let mut input_data = bytes_requested.to_be_bytes().to_vec();
        input_data.extend(personalization);
        let input = Bytes::from(input_data);
        
        let gas_limit = 10000;
        
        let result = precompiles.run(&mut context, &rng_address, &input, gas_limit);
        
        assert!(result.is_ok(), "SeismicPrecompiles should successfully route RNG call");
        
        let interpreter_result = result.unwrap().expect("Should return Some(InterpreterResult)");
        let output_bytes = interpreter_result.output;
        
        assert_eq!(output_bytes.len(), 32, "RNG output should be 32 bytes");
        assert_eq!(output_bytes, Bytes::from(hex!("6205fa1fc78e42116f1b370e200a867805679032f64ab68256ae59d678dc441d")), 
                  "RNG precompile should return successfully");
        
        let gas_used = gas_limit - interpreter_result.gas.remaining();
        assert!(gas_used >= 3500 && gas_used <= 3600, 
                "Gas used should be in expected range, got {}", gas_used);
        
        let result2 = precompiles.run(&mut context, &rng_address, &input, gas_limit);
        assert!(result2.is_ok(), "Second RNG call should succeed");
        
        let interpreter_result2 = result2.unwrap().expect("Should return Some(InterpreterResult)");
        let output_bytes2 = interpreter_result2.output;
        
        assert_eq!(output_bytes2.len(), 32, "Second RNG output should be 32 bytes");
        
        let gas_used2 = gas_limit - interpreter_result2.gas.remaining();
        assert!(gas_used2 < gas_used, 
                "Second call should use less gas, used {} vs first call {}", gas_used2, gas_used);
        
        assert_ne!(output_bytes, output_bytes2, 
                  "Subsequent RNG calls should return different outputs");
    }
    
    #[test]
    fn test_seismic_precompiles_warm_addresses() {
        // Setup the SeismicPrecompiles
        let precompiles = SeismicPrecompiles::<SeismicContext<EmptyDB>>::new_with_spec(SeismicSpecId::MERCURY);
        
        // Get all warm addresses
        let warm_addresses: Vec<Address> = precompiles.warm_addresses().collect();
        
        // Verify RNG address is included
        let rng_address = *precompiles.stateful_precompiles.addresses().next().expect("RNG precompile address should exist");
        assert!(warm_addresses.contains(&rng_address), 
                "warm_addresses() should include RNG precompile address");
        
        // Verify standard precompile addresses are included
        let ecrecover_address = Address::from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert!(warm_addresses.contains(&ecrecover_address), 
                "warm_addresses() should include standard precompile addresses");
        
        // Verify we have the expected number of warm addresses
        // This should be the number of standard precompiles + number of stateful precompiles
        assert!(warm_addresses.len() > 10, 
                "warm_addresses() should return multiple addresses, got {}", warm_addresses.len());
    }
}

