use auto_impl::auto_impl;
use revm::{
    context::TxEnv,
    context_interface::transaction::Transaction,
    handler::SystemCallTx,
    primitives::{Address, Bytes, TxKind, B256, U256},
};

#[auto_impl(&, &mut, Box, Arc)]
pub trait SeismicTxTr: Transaction {
    /// tx hash of the transaction
    fn tx_hash(&self) -> B256;

    /// Whether this transaction failed calldata decryption.
    fn decryption_failed(&self) -> bool;
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SeismicTransaction<T: Transaction> {
    pub base: T,
    /// tx hash of the transaction. Used for domain separation in the RNG.
    pub tx_hash: B256,
    /// Whether this transaction failed decryption. Used for handling execution and metering.
    pub decryption_failed: bool,
}

impl<T: Transaction> SeismicTransaction<T> {
    pub fn new(base: T) -> Self {
        Self {
            base,
            tx_hash: B256::ZERO,
            decryption_failed: false,
        }
    }

    pub fn with_tx_hash(mut self, tx_hash: B256) -> Self {
        self.tx_hash = tx_hash;
        self
    }

    pub fn with_decryption_failed(mut self, failed: bool) -> Self {
        self.decryption_failed = failed;
        self
    }
}

impl<T: Transaction> std::ops::Deref for SeismicTransaction<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl<T: Transaction> std::ops::DerefMut for SeismicTransaction<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

impl<T: Transaction> From<T> for SeismicTransaction<T> {
    fn from(base: T) -> Self {
        Self::new(base)
    }
}

impl Default for SeismicTransaction<TxEnv> {
    fn default() -> Self {
        Self {
            base: TxEnv::default(),
            tx_hash: B256::ZERO,
            decryption_failed: false,
        }
    }
}

impl<T: Transaction> Transaction for SeismicTransaction<T> {
    type AccessListItem<'a>
        = T::AccessListItem<'a>
    where
        T: 'a;
    type Authorization<'a>
        = T::Authorization<'a>
    where
        T: 'a;

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

    fn access_list(&self) -> Option<impl Iterator<Item = Self::AccessListItem<'_>>> {
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

    fn authorization_list(&self) -> impl Iterator<Item = Self::Authorization<'_>> {
        self.base.authorization_list()
    }
}

impl<T: Transaction> SeismicTxTr for SeismicTransaction<T> {
    fn tx_hash(&self) -> B256 {
        self.tx_hash
    }

    fn decryption_failed(&self) -> bool {
        self.decryption_failed
    }
}

impl<TX: Transaction + SystemCallTx> SystemCallTx for SeismicTransaction<TX> {
    fn new_system_tx_with_caller(
        caller: Address,
        system_contract_address: Address,
        data: Bytes,
    ) -> Self {
        SeismicTransaction::new(TX::new_system_tx_with_caller(
            caller,
            system_contract_address,
            data,
        ))
    }
}
