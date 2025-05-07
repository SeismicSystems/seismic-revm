pub mod aes_gcm_enc;
pub use aes_gcm_enc::precompile_encrypt;

pub mod aes_gcm_dec;
pub use aes_gcm_dec::precompile_decrypt;

mod common;
