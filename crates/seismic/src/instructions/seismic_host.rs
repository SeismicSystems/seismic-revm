use revm::{
    context::ContextTr, context_interface::{
        context::ContextError, journaled_state::AccountLoad, Database
    }, database::EmptyDB, interpreter::{Host, SStoreResult, SelfDestructResult, StateLoad}, primitives::{Address, Bytes, Log, B256, U256}
};

use crate::api::exec::SeismicContextTr;

// Extend Host with an associated Db type and error() method
pub trait SeismicHost: Host {
    type Db: Database;

    fn ctx_error(&mut self) -> &mut Result<(), ContextError<<Self::Db as Database>::Error>>;
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
}

impl SeismicDummyHost {
    pub fn new() -> Self {
        Self {
            ctx_result: Ok(()),
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
        U256::ZERO
    }

    fn blob_gasprice(&self) -> U256 {
        U256::ZERO
    }

    fn gas_limit(&self) -> U256 {
        U256::ZERO
    }

    fn difficulty(&self) -> U256 {
        U256::ZERO
    }

    fn prevrandao(&self) -> Option<U256> {
        None
    }

    fn block_number(&self) -> u64 {
        0
    }

    fn timestamp(&self) -> U256 {
        U256::ZERO
    }

    fn beneficiary(&self) -> Address {
        Address::ZERO
    }

    fn chain_id(&self) -> U256 {
        U256::ZERO
    }

    fn effective_gas_price(&self) -> U256 {
        U256::ZERO
    }

    fn caller(&self) -> Address {
        Address::ZERO
    }

    fn blob_hash(&self, _number: usize) -> Option<U256> {
        None
    }

    fn max_initcode_size(&self) -> usize {
        0
    }

    fn block_hash(&mut self, _number: u64) -> Option<B256> {
        None
    }

    fn selfdestruct(
        &mut self,
        _address: Address,
        _target: Address,
    ) -> Option<StateLoad<SelfDestructResult>> {
        None
    }

    fn log(&mut self, _log: Log) {}

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
        _address: Address,
        _key: U256,
        _value: U256,
    ) -> Option<StateLoad<SStoreResult>> {
        None
    }

    fn sload(&mut self, _address: Address, _key: U256) -> Option<StateLoad<U256>> {
        None
    }

    fn cload(&mut self, _address: Address, _key: U256) -> Option<StateLoad<U256>> {
        None
    }

    fn tstore(&mut self, _address: Address, _key: U256, _value: U256) {}

    fn tload(&mut self, _address: Address, _key: U256) -> U256 {
        U256::ZERO
    }

    fn balance(&mut self, _address: Address) -> Option<StateLoad<U256>> {
        None
    }

    fn load_account_delegated(&mut self, _address: Address) -> Option<StateLoad<AccountLoad>> {
        None
    }

    fn load_account_code(&mut self, _address: Address) -> Option<StateLoad<Bytes>> {
        None
    }

    fn load_account_code_hash(&mut self, _address: Address) -> Option<StateLoad<B256>> {
        None
    }
}
