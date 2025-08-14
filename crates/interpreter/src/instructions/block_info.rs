use crate::{
    gas,
    interpreter::Interpreter,
    interpreter_types::{InterpreterTypes, LoopControl, RuntimeFlag, StackTr},
    Host,
};
use primitives::{hardfork::SpecId::*, U256};

/// EIP-1344: ChainID opcode
pub fn chainid<WIRE: InterpreterTypes, H: Host + ?Sized>(
    interpreter: &mut Interpreter<WIRE>,
    host: &mut H,
) {
    check!(interpreter, ISTANBUL);
    gas!(interpreter, gas::BASE);
    push!(interpreter, host.chain_id());
}

pub fn coinbase<WIRE: InterpreterTypes, H: Host + ?Sized>(
    interpreter: &mut Interpreter<WIRE>,
    host: &mut H,
) {
    gas!(interpreter, gas::BASE);
    push!(interpreter, host.beneficiary().into_word().into());
}

pub fn timestamp<WIRE: InterpreterTypes, H: Host + ?Sized>(
    interpreter: &mut Interpreter<WIRE>,
    host: &mut H,
) {
    gas!(interpreter, gas::BASE);
    #[cfg(not(feature = "timestamp_in_seconds"))]
    {
        // Host returns milliseconds, convert to seconds for EVM compatibility
        let timestamp_seconds = host.timestamp() / U256::from(1000);
        push!(interpreter, timestamp_seconds);
    }

    #[cfg(feature = "timestamp_in_seconds")]
    {
        // Host returns seconds, use as is
        push!(interpreter, host.timestamp());
    }
}

pub fn timestamp_milliseconds<WIRE: InterpreterTypes, H: Host + ?Sized>(
    interpreter: &mut Interpreter<WIRE>,
    host: &mut H,
) {
    gas!(interpreter, gas::BASE);

    #[cfg(feature = "timestamp_in_seconds")]
    {
        // Host returns seconds, convert to milliseconds for reth compatibility
        let timestamp_ms = host.timestamp() * U256::from(1000);
        push!(interpreter, timestamp_ms);
    }

    #[cfg(not(feature = "timestamp_in_seconds"))]
    {
        // Host returns milliseconds, use as is
        push!(interpreter, host.timestamp());
    }
}

pub fn block_number<WIRE: InterpreterTypes, H: Host + ?Sized>(
    interpreter: &mut Interpreter<WIRE>,
    host: &mut H,
) {
    gas!(interpreter, gas::BASE);
    push!(interpreter, U256::from(host.block_number()));
}

pub fn difficulty<WIRE: InterpreterTypes, H: Host + ?Sized>(
    interpreter: &mut Interpreter<WIRE>,
    host: &mut H,
) {
    gas!(interpreter, gas::BASE);
    if interpreter.runtime_flag.spec_id().is_enabled_in(MERGE) {
        // Unwrap is safe as this fields is checked in validation handler.
        push!(interpreter, host.prevrandao().unwrap());
    } else {
        push!(interpreter, host.difficulty());
    }
}

pub fn gaslimit<WIRE: InterpreterTypes, H: Host + ?Sized>(
    interpreter: &mut Interpreter<WIRE>,
    host: &mut H,
) {
    gas!(interpreter, gas::BASE);
    push!(interpreter, host.gas_limit());
}

/// EIP-3198: BASEFEE opcode
pub fn basefee<WIRE: InterpreterTypes, H: Host + ?Sized>(
    interpreter: &mut Interpreter<WIRE>,
    host: &mut H,
) {
    check!(interpreter, LONDON);
    gas!(interpreter, gas::BASE);
    push!(interpreter, host.basefee());
}

/// EIP-7516: BLOBBASEFEE opcode
pub fn blob_basefee<WIRE: InterpreterTypes, H: Host + ?Sized>(
    interpreter: &mut Interpreter<WIRE>,
    host: &mut H,
) {
    check!(interpreter, CANCUN);
    gas!(interpreter, gas::BASE);
    push!(interpreter, host.blob_gasprice());
}
