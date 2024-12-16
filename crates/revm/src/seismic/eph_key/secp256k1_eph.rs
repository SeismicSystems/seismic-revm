use crate::{
    primitives::{db::Database, Address, Bytes}, seismic::rng, ContextPrecompile, ContextStatefulPrecompile, InnerEvmContext
};
use secp256k1::{
    ecdsa::{RecoverableSignature, RecoveryId},
    Message, SECP256K1,
    generate_keypair,
};
use revm_precompile::{
    u64_to_address, Error as REVM_ERROR, PrecompileError, PrecompileOutput, PrecompileResult,
};
use crate::seismic::rng::precompile::get_leaf_rng;
use crate::precompile::Error as PCError;

pub fn gen_secp256k1_sk<DB: Database>(
    input: &Bytes,
    gas_limit: u64,
    evmctx: &mut InnerEvmContext<DB>,
) -> PrecompileResult {
    let gas_used = 100; // TODO: refine this constant
    if gas_used > gas_limit {
        return Err(REVM_ERROR::OutOfGas.into());
    }

    // get a leaf_rng 
    let mut leaf_rng = get_leaf_rng(input, evmctx).map_err(|e| PCError::Other(e.to_string()))?;

    // generate the keys
    let (secret_key, public_key) = generate_keypair(&mut leaf_rng);
    let sk_bytes: [u8; 32] = secret_key.secret_bytes();
    Ok(PrecompileOutput::new(gas_used, sk_bytes.into()))
}


// fn rng_call<DB: Database>(
//     input: &Bytes,
//     gas_limit: u64,
//     evmctx: &mut InnerEvmContext<DB>,
// ) -> Result<Bytes, PrecompileError> {
//         let gas_used = 100; // TODO: refine this constant
//         if gas_used > gas_limit {
//             return Err(REVM_ERROR::OutOfGas.into());
//         }

//         let pers = input.as_ref(); // pers is the personalized entropy added by the caller

//         // Get the random bytes
//         // TODO: evaluate if this is good, ex if the tx_hash is correct
//         let env = evmctx.env().clone();
//         let root_rng = &mut evmctx.kernel.root_rng;
//         let tx_hash = hash_tx_env(&env.tx);
//         root_rng.append_tx(tx_hash);
//         let mut leaf_rng = match root_rng.fork(&env, pers) {
//             Ok(rng) => rng,
//             Err(_err) => {
//                 return Err(PrecompileError::Other("Rng fork failed".to_string()).into());
//             }
//         };

//         let mut rng_bytes = [0u8; 32];
//         leaf_rng.fill_bytes(&mut rng_bytes);
//         let output = Bytes::from(rng_bytes);

//         Ok(output)
//     }
    