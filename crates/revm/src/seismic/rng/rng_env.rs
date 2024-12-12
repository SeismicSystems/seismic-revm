use tiny_keccak::{Hasher, TupleHash};
use alloy_primitives::{B256, U256};

// TODO: clean up this type
#[derive(Clone, Debug, Copy)]
pub struct RngEnv {
    pub block_number: U256,
    pub tx_env_hash: B256,
}

impl RngEnv {
    pub fn new(block_number: U256, tx_env_hash: B256) -> Self {
        Self { block_number, tx_env_hash }
    }

    pub fn hash(&self) -> [u8; 32] {
        // return [0u8; 32];
        todo!()
        // hash_rng_env([self.block_number.to_be_bytes().as_ref()])
    }
    
}

pub const VRF_KEY_CONTEXT: &[u8] = b"seismic VRF_KEY_CONTEXT";

// Hash elements of the block context to derive a vrf key for the block
pub fn hash_rng_env<'a, C>(context: C) -> [u8; 32]
where
    C: IntoIterator<Item = &'a [u8]> + 'a,
{
    let mut h = TupleHash::v256(VRF_KEY_CONTEXT);
    for item in context.into_iter() {
        h.update(item);
    }
    let mut key_pair_id = [0u8; 32];
    h.finalize(&mut key_pair_id);

    key_pair_id
}
