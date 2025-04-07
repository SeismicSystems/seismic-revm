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
//pub mod rng;
pub mod secp256k1_sign;
pub mod stateful_precompile;
pub use stateful_precompile::StatefulPrecompiles;

use crate::SeismicSpecId;
use once_cell::race::OnceBox;
use revm::{
    context::Cfg,
    context_interface::ContextTr,
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
pub struct SeismicPrecompiles<CTX: ContextTr> {
    inner: EthPrecompiles,
    stateful_precompiles: StatefulPrecompiles<CTX>,
}

impl <CTX: ContextTr> SeismicPrecompiles<CTX> {
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
pub fn mercury<CTX: ContextTr>() -> (&'static Precompiles, StatefulPrecompiles<CTX>) {
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
    let stateful_precompiles = StatefulPrecompiles::new();
    //stateful_precompiles.extend(rng::precompiles::<CTX>().map(|p| (p.0, p.1)));
    (regular_precompiles, stateful_precompiles)
}

impl<CTX> PrecompileProvider<CTX> for SeismicPrecompiles<CTX>
where
    CTX: ContextTr,
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

impl<CTX: ContextTr> Default for SeismicPrecompiles<CTX>
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
    use revm::database::EmptyDB;
    
    
    
    use crate::{DefaultSeismic,SeismicContext};
    

    #[test]
    fn test_cancun_precompiles_in_mercury() {
        let context = SeismicContext::<EmptyDB>::seismic();
        assert_eq!(mercury::<SeismicContext<EmptyDB>>().0.difference(Precompiles::prague()).len(), 6)
    }

    #[test]
    fn test_default_precompiles_is_latest() {
        let context = SeismicContext::<EmptyDB>::seismic();
        let latest = SeismicPrecompiles::<SeismicContext<EmptyDB>>::new_with_spec(SeismicSpecId::default())
            .inner
            .precompiles;
        let default = SeismicPrecompiles::<SeismicContext<EmptyDB>>::default().inner.precompiles;
        assert_eq!(latest.len(), default.len());

        let intersection = default.intersection(latest);
        assert_eq!(intersection.len(), latest.len())
    }
}

