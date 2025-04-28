use revm::{
    context::ContextTr,
    context_interface::{context::ContextError, journaled_state::AccountLoad, Database},
    database::EmptyDB,
    interpreter::{host::DummyHost, Host, SStoreResult, SelfDestructResult, StateLoad},
    primitives::{Address, Bytes, FlaggedStorage, Log, B256, U256},
};

use crate::{api::exec::SeismicContextTr, SeismicHaltReason};

// Extend Host with an associated Db type and error() method
pub trait SeismicHost: Host {
    type Db: Database;

    fn ctx_error(&mut self) -> &mut Result<(), ContextError<<Self::Db as Database>::Error>>;
    fn set_ctx_error<E>(&mut self, error: E)
    where
        E: Into<ContextError<<Self::Db as Database>::Error>>,
    {
        *self.ctx_error() = Err(error.into());
    }

    fn set_halt_reason(&mut self, reason: SeismicHaltReason) {
        *self.ctx_error() = Err(ContextError::Custom(reason.to_string()));
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
}

pub struct SeismicDummyHost {
    ctx_result: Result<(), ContextError<<EmptyDB as Database>::Error>>,
    dummy_host: DummyHost,
}

impl SeismicDummyHost {
    pub fn new() -> Self {
        Self {
            ctx_result: Ok(()),
            dummy_host: DummyHost,
        }
    }
}

impl SeismicHost for SeismicDummyHost {
    type Db = EmptyDB;

    fn ctx_error(&mut self) -> &mut Result<(), ContextError<<Self::Db as Database>::Error>> {
        &mut self.ctx_result
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

    fn block_number(&self) -> u64 {
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
        _address: Address,
        _key: U256,
        _value: U256,
    ) -> Option<StateLoad<SStoreResult>> {
        None
    }

    fn sstore(
        &mut self,
        address: Address,
        key: U256,
        value: U256,
    ) -> Option<StateLoad<SStoreResult>> {
        self.dummy_host.sstore(address, key, value)
    }

    fn sload(&mut self, address: Address, key: U256) -> Option<StateLoad<FlaggedStorage>> {
        self.dummy_host.sload(address, key)
    }

    fn cload(&mut self, _address: Address, _key: U256) -> Option<StateLoad<FlaggedStorage>> {
        None
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
}
