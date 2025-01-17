//! Handler related to Seismic chain

use super::{
    eph_key::{aes_gcm_dec, aes_gcm_enc, ecdh_derive_sym_key, hkdf_derive_sym_key},
    kernel::new_test_kernel_box,
};
use crate::{
    handler::register::EvmHandler,
    primitives::{db::Database, spec_to_generic, EVMError, Spec, SpecId},
    seismic::rng::precompile::RngPrecompile,
    Context, ContextPrecompiles, Frame,
};
use revm_interpreter::{opcode::InstructionTables, Host, InterpreterAction, SharedMemory};
use revm_precompile::{secp256r1, PrecompileSpecId};
use std::sync::Arc;

pub fn seismic_handle_register<DB: Database, EXT>(handler: &mut EvmHandler<'_, EXT, DB>) {
    spec_to_generic!(handler.cfg.spec_id, {
        handler.validation.tx_against_state = Arc::new(validate_tx_against_state::<SPEC, EXT, DB>);
        handler.execution.execute_frame = Arc::new(execute_frame::<SPEC, EXT, DB>);
        handler.pre_execution.load_precompiles = Arc::new(load_precompiles::<SPEC, EXT, DB>);
    });
}

/// We use this hook to make sure ctx is initialized for RNG purpose
fn validate_tx_against_state<SPEC: Spec, EXT, DB: Database>(
    context: &mut Context<EXT, DB>,
) -> Result<(), EVMError<DB::Error>> {
    if context.evm.kernel.ctx_is_empty() {
        context.evm.kernel = new_test_kernel_box(context.env())
    }
    crate::handler::mainnet::validate_tx_against_state::<SPEC, EXT, DB>(context)
}

// Hook onto callframe to append domain-separation to our RNG
#[inline]
fn execute_frame<SPEC: Spec, EXT, DB: Database>(
    frame: &mut Frame,
    shared_memory: &mut SharedMemory,
    instruction_tables: &InstructionTables<'_, Context<EXT, DB>>,
    context: &mut Context<EXT, DB>,
) -> Result<InterpreterAction, EVMError<DB::Error>> {
    let kernel = &mut context.evm.kernel;
    let tx_hash = kernel.ctx_ref().unwrap().transaction_hash;

    // // TODO: consider if we should apply local entropy here
    // kernel.maybe_append_entropy();
    
    // TODO: test this works as expected
    // apply tx_hash to the rng
    let rng = kernel.rng_mut_ref();
    rng.append_tx(&tx_hash);
   
    crate::handler::mainnet::execute_frame::<SPEC, EXT, DB>(
        frame,
        shared_memory,
        instruction_tables,
        context,
    )
}

// Load precompiles for Seismic chain.
#[inline]
pub fn load_precompiles<SPEC: Spec, EXT, DB: Database>() -> ContextPrecompiles<DB> {
    let mut precompiles = ContextPrecompiles::new(PrecompileSpecId::from_spec_id(SPEC::SPEC_ID));

    if SPEC::enabled(SpecId::MERCURY) {
        // extend with PrecompileWithAddress
        precompiles.extend([
            secp256r1::P256VERIFY,
            ecdh_derive_sym_key::PRECOMPILE,
            hkdf_derive_sym_key::PRECOMPILE,
            aes_gcm_enc::PRECOMPILE,
            aes_gcm_dec::PRECOMPILE,
        ]);
        // extend with ContextPrecompile<DB>
        precompiles.extend([RngPrecompile::address_and_precompile::<DB>()]);
    }
    precompiles
}
