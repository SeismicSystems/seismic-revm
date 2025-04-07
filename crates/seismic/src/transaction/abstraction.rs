use auto_impl::auto_impl;
use revm::{
    context::TxEnv,
    context_interface::transaction::Transaction,
    primitives::{Address, Bytes, TxKind, B256, U256},
};
use std::vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Indicates the runtime context for the kernel.
/// Use `Simulation` for endpoints (like eth_call) that need unique entropy,
/// and `Execution` for normal transaction execution (used for both tests and production).
pub enum RngMode {
    Simulation,
    Execution,
}

#[auto_impl(&, &mut, Box, Arc)]
pub trait SeismicTxTr: Transaction {
    /// tx hash of the transaction
    fn tx_hash(&self) -> Option<B256>;

    /// rng mode for this transaction 
    fn rng_mode(&self) -> RngMode;
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SeismicTransaction<T: Transaction> {
    pub base: T,
    /// tx hash of the transaction. Used for domain separation in the RNG.
    pub tx_hash: Option<B256>,
    pub rng_mode: RngMode,
}

impl<T: Transaction> SeismicTransaction<T> {
    pub fn new(base: T) -> Self {
        Self {
            base,
            tx_hash: None,
            rng_mode: RngMode::Execution,
        }
    }
}

impl Default for SeismicTransaction<TxEnv> {
    fn default() -> Self {
        Self {
            base: TxEnv::default(),
            tx_hash: None,
            rng_mode: RngMode::Execution,
        }
    }
}

impl<T: Transaction> Transaction for SeismicTransaction<T> {
    type AccessListItem = T::AccessListItem;
    type Authorization = T::Authorization;

    fn tx_type(&self) -> u8 {
        self.base.tx_type()
    }

    fn caller(&self) -> Address {
        self.base.caller()
    }

    fn gas_limit(&self) -> u64 {
        self.base.gas_limit()
    }

    fn value(&self) -> U256 {
        self.base.value()
    }

    fn input(&self) -> &Bytes {
        self.base.input()
    }

    fn nonce(&self) -> u64 {
        self.base.nonce()
    }

    fn kind(&self) -> TxKind {
        self.base.kind()
    }

    fn chain_id(&self) -> Option<u64> {
        self.base.chain_id()
    }

    fn access_list(&self) -> Option<impl Iterator<Item = &Self::AccessListItem>> {
        self.base.access_list()
    }

    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        self.base.max_priority_fee_per_gas()
    }

    fn max_fee_per_gas(&self) -> u128 {
        self.base.max_fee_per_gas()
    }

    fn gas_price(&self) -> u128 {
        self.base.gas_price()
    }

    fn blob_versioned_hashes(&self) -> &[B256] {
        self.base.blob_versioned_hashes()
    }

    fn max_fee_per_blob_gas(&self) -> u128 {
        self.base.max_fee_per_blob_gas()
    }

    fn effective_gas_price(&self, base_fee: u128) -> u128 {
        self.base.effective_gas_price(base_fee)
    }

    fn authorization_list_len(&self) -> usize {
        self.base.authorization_list_len()
    }

    fn authorization_list(&self) -> impl Iterator<Item = &Self::Authorization> {
        self.base.authorization_list()
    }
}

impl<T: Transaction> SeismicTxTr for SeismicTransaction<T> {
    fn tx_hash(&self) -> Option<B256> {
        self.tx_hash
    }

    fn rng_mode(&self) -> RngMode {
        self.rng_mode
    }
}

