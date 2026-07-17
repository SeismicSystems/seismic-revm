use revm::{
    context::{host::LoadError, journaled_state::AccountInfoLoad, ContextTr},
    context_interface::{
        context::ContextError, journaled_state::AccountLoad, transaction::Transaction, Database,
    },
    database::EmptyDB,
    interpreter::{host::DummyHost, Host, SStoreResult, SelfDestructResult, StateLoad},
    primitives::{Address, Bytes, Log, StorageKey, StorageValue, B256, U256},
};

use crate::api::exec::SeismicContextTr;

// Extend Host with an associated Db type and error() method
pub trait SeismicHost: Host {
    type Db: Database;

    fn ctx_error(&mut self) -> &mut Result<(), ContextError<<Self::Db as Database>::Error>>;

    /// Transaction type byte (EIP-2718) of the currently executing transaction.
    /// A Seismic (encrypted-calldata) transaction has type `74` (`0x4A`).
    fn tx_type(&self) -> u8;

    fn set_ctx_error<E>(&mut self, error: E)
    where
        E: Into<ContextError<<Self::Db as Database>::Error>>,
    {
        *self.ctx_error() = Err(error.into());
    }
}

impl<CTX> SeismicHost for CTX
where
    CTX: SeismicContextTr + Host,
{
    type Db = CTX::Db;

    fn ctx_error(&mut self) -> &mut Result<(), ContextError<<Self::Db as Database>::Error>> {
        <Self as ContextTr>::error(self)
    }

    fn tx_type(&self) -> u8 {
        <Self as ContextTr>::tx(self).tx_type()
    }
}

pub struct SeismicDummyHost {
    ctx_result: Result<(), ContextError<<EmptyDB as Database>::Error>>,
    dummy_host: DummyHost,
    tx_type: u8,
}

impl Default for SeismicDummyHost {
    fn default() -> Self {
        Self {
            ctx_result: Ok(()),
            dummy_host: DummyHost,
            tx_type: 0,
        }
    }
}

impl SeismicDummyHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the transaction type reported by [`SeismicHost::tx_type`].
    pub fn with_tx_type(mut self, tx_type: u8) -> Self {
        self.tx_type = tx_type;
        self
    }
}

impl SeismicHost for SeismicDummyHost {
    type Db = EmptyDB;

    fn ctx_error(&mut self) -> &mut Result<(), ContextError<<Self::Db as Database>::Error>> {
        &mut self.ctx_result
    }

    fn tx_type(&self) -> u8 {
        self.tx_type
    }
}

impl Host for SeismicDummyHost {
    fn basefee(&self) -> U256 {
        self.dummy_host.basefee()
    }

    fn blob_gasprice(&self) -> U256 {
        self.dummy_host.blob_gasprice()
    }

    fn gas_limit(&self) -> U256 {
        self.dummy_host.gas_limit()
    }

    fn difficulty(&self) -> U256 {
        self.dummy_host.difficulty()
    }

    fn prevrandao(&self) -> Option<U256> {
        self.dummy_host.prevrandao()
    }

    fn block_number(&self) -> U256 {
        self.dummy_host.block_number()
    }

    fn timestamp(&self) -> U256 {
        self.dummy_host.timestamp()
    }

    fn beneficiary(&self) -> Address {
        self.dummy_host.beneficiary()
    }

    fn chain_id(&self) -> U256 {
        self.dummy_host.chain_id()
    }

    fn effective_gas_price(&self) -> U256 {
        self.dummy_host.effective_gas_price()
    }

    fn caller(&self) -> Address {
        self.dummy_host.caller()
    }

    fn blob_hash(&self, number: usize) -> Option<U256> {
        self.dummy_host.blob_hash(number)
    }

    fn max_initcode_size(&self) -> usize {
        self.dummy_host.max_initcode_size()
    }

    fn block_hash(&mut self, number: u64) -> Option<B256> {
        self.dummy_host.block_hash(number)
    }

    fn selfdestruct(
        &mut self,
        address: Address,
        target: Address,
    ) -> Option<StateLoad<SelfDestructResult>> {
        self.dummy_host.selfdestruct(address, target)
    }

    fn log(&mut self, _log: Log) {
        self.dummy_host.log(_log)
    }

    fn cstore(
        &mut self,
        address: Address,
        key: U256,
        value: U256,
        skip_cold_load: bool,
    ) -> Result<StateLoad<SStoreResult>, LoadError> {
        self.dummy_host.cstore(address, key, value, skip_cold_load)
    }

    fn cload(
        &mut self,
        address: Address,
        key: U256,
        skip_cold_load: bool,
    ) -> Result<StateLoad<U256>, LoadError> {
        self.dummy_host.cload(address, key, skip_cold_load)
    }

    fn sstore(
        &mut self,
        address: Address,
        key: U256,
        value: U256,
    ) -> Option<StateLoad<SStoreResult>> {
        self.dummy_host.sstore(address, key, value)
    }

    fn sload(&mut self, address: Address, key: U256) -> Option<StateLoad<U256>> {
        self.dummy_host.sload(address, key)
    }

    fn tstore(&mut self, address: Address, key: U256, value: U256) {
        self.dummy_host.tstore(address, key, value)
    }

    fn tload(&mut self, address: Address, key: U256) -> U256 {
        self.dummy_host.tload(address, key)
    }

    fn balance(&mut self, address: Address) -> Option<StateLoad<U256>> {
        self.dummy_host.balance(address)
    }

    fn load_account_delegated(&mut self, address: Address) -> Option<StateLoad<AccountLoad>> {
        self.dummy_host.load_account_delegated(address)
    }

    fn load_account_code(&mut self, address: Address) -> Option<StateLoad<Bytes>> {
        self.dummy_host.load_account_code(address)
    }

    fn load_account_code_hash(&mut self, address: Address) -> Option<StateLoad<B256>> {
        self.dummy_host.load_account_code_hash(address)
    }

    fn load_account_info_skip_cold_load(
        &mut self,
        _address: Address,
        _load_code: bool,
        _skip_cold_load: bool,
    ) -> Result<AccountInfoLoad<'_>, LoadError> {
        Err(LoadError::DBError)
    }

    fn sstore_skip_cold_load(
        &mut self,
        _address: Address,
        _key: StorageKey,
        _value: StorageValue,
        _skip_cold_load: bool,
    ) -> Result<StateLoad<SStoreResult>, LoadError> {
        Err(LoadError::DBError)
    }

    fn sload_skip_cold_load(
        &mut self,
        _address: Address,
        _key: StorageKey,
        _skip_cold_load: bool,
    ) -> Result<StateLoad<StorageValue>, LoadError> {
        Err(LoadError::DBError)
    }
}
