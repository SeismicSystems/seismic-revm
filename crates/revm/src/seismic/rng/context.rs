use tiny_keccak::{Hasher, TupleHash};

// TODO: clean up this context type
pub type BlockHeader = [u8; 32];
#[derive(Clone, Debug, Copy)]
pub struct Context { // potentially could hold other contextual info
    pub header: BlockHeader,
}

impl Context {
    pub fn new(header: BlockHeader) -> Self {
        Self { header }
    }
    
}

pub const VRF_KEY_CONTEXT: &[u8] = b"seismic VRF_KEY_CONTEXT";

// Hash elements of the block context to derive a vrf key for the block
pub fn hash_context<'a, C>(context: C) -> [u8; 32]
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
