use tiny_keccak::{Hasher, TupleHash};


// NOTE: used instead of Oasis Core hash type
pub type Hash = [u8; 32];

pub const KEY_PAIR_ID_CONTEXT: &[u8] = b"seismic KEY_PAIR_ID_CONTEXT";

// Essentially a hash of the context to derive the VRF key
pub fn get_key_pair_id<'a, C>(context: C) -> [u8; 32]
where
    C: IntoIterator<Item = &'a [u8]> + 'a,
{
    let mut h = TupleHash::v256(KEY_PAIR_ID_CONTEXT);
    for item in context.into_iter() {
        h.update(item);
    }
    let mut key_pair_id = [0u8; 32];
    h.finalize(&mut key_pair_id);

    key_pair_id
}

