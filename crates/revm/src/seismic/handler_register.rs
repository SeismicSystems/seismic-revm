//! Handler related to Seismic chain

use crate::{
    handler::register::EvmHandler,
    primitives::{db::Database, spec_to_generic, EVMError, Spec, SpecId},
    seismic::{ Kernel},
    // seismic::rng::precompile::RNG_PRECOMPILE,
    Context, ContextPrecompiles, Frame,
};
use revm_interpreter::{opcode::InstructionTables, Host, InterpreterAction, SharedMemory};
use revm_precompile::{secp256r1, Address, PrecompileSpecId, PrecompileWithAddress, StatefulPrecompile};
use std::sync::Arc;

use super::rng;


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
        context.evm.kernel = Kernel::new(context.env())
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
    context.evm.inner.kernel.root_rng.append_subcontext();
    crate::handler::mainnet::execute_frame::<SPEC, EXT, DB>(
        frame,
        shared_memory,
        instruction_tables,
        context,
    )
}


use crate::primitives::{Precompile, StatefulPrecompileArc};
use crate::seismic::rng::precompile::RngPrecompile;
use crate::ContextStatefulPrecompile;
use crate::db::EmptyDB;
use revm_precompile::u64_to_address;
use crate::ContextPrecompile;

// Load precompiles for Seismic chain.
#[inline]
pub fn load_precompiles<SPEC: Spec, EXT, DB: Database>() -> ContextPrecompiles<DB> {
    let mut precompiles = ContextPrecompiles::new(PrecompileSpecId::from_spec_id(SPEC::SPEC_ID));
    let addr: Address = u64_to_address(100);


    let precompile= ContextPrecompile::ContextStateful(Arc::new(RngPrecompile));
    // let addr = RNG_PRECOMPILE.0;
    // let rng_context_precompile = RNG_PRECOMPILE.1;
    
    
    // let stateful_precompile: dyn StatefulPrecompile = rng_context_precompile;
    // let stateful_precompile_arc: StatefulPrecompileArc = Arc::new(stateful_precompile);
    // let precompile_with_address: PrecompileWithAddress = PrecompileWithAddress(addr, stateful_precompile_arc);

    if SPEC::enabled(SpecId::MERCURY) {
        // extend with PrecompileWithAddress
        precompiles.extend([
            secp256r1::P256VERIFY,
        ]);
        // extend with ContextPrecompile<DbB>
        precompiles.extend([
            // EIP-7212: secp256r1 P256verify
            // secp256r1::P256VERIFY,
            (addr, precompile),
        ]);
       
    }
    precompiles
}
