use super::{Journal, JournalEntry};
use context_interface::JournalTr;
use database::InMemoryDB;
use database_interface::DatabaseCommit;
use primitives::{hardfork::SpecId, Address, FlaggedStorage, U256};
use state::AccountInfo;

fn journal() -> Journal<InMemoryDB, JournalEntry> {
    let mut db = InMemoryDB::default();
    db.insert_account_info(
        Address::repeat_byte(1),
        AccountInfo {
            nonce: 1,
            ..Default::default()
        },
    );
    let mut journal = Journal::new(db);
    journal.set_spec_id(SpecId::MERCURY);
    journal
}

#[test]
fn system_reads_load_cold_without_touching_or_initializing() {
    let mut journal = journal();
    let address = Address::repeat_byte(1);
    let key = U256::from(7);
    for _ in 0..2 {
        let read = journal.system_storage(address, key).unwrap();
        assert_eq!(read.data, U256::ZERO);
        assert!(!read.is_private);
        assert!(read.is_cold);
        let account = &journal.inner.state[&address];
        assert!(!account.is_touched());
        assert!(account.is_cold_transaction_id(journal.transaction_id));
        assert!(account.storage[&key].is_cold_transaction_id(journal.transaction_id));
        assert!(journal.inner.journal.is_empty());
    }
    assert!(journal.load_account(address).unwrap().is_cold);
    assert!(!journal.load_account(address).unwrap().is_cold);
    assert!(journal.sload(address, key).unwrap().is_cold);
    assert!(!journal.sload(address, key).unwrap().is_cold);
}

#[test]
fn system_access_preserves_existing_warmth_and_protocol_warmth() {
    let mut journal = journal();
    let address = Address::repeat_byte(1);
    let key = U256::from(7);
    journal.load_account(address).unwrap();
    journal.sload(address, key).unwrap();
    assert!(!journal.system_storage(address, key).unwrap().is_cold);
    journal
        .system_store(address, key, FlaggedStorage::new(U256::from(1), false))
        .unwrap();
    assert!(!journal.sload(address, key).unwrap().is_cold);
    assert!(!journal.load_account(address).unwrap().is_cold);

    let coinbase = Address::repeat_byte(2);
    journal.warm_coinbase_account(coinbase);
    journal.system_storage(coinbase, key).unwrap();
    assert!(!journal.load_account(coinbase).unwrap().is_cold);
}

#[test]
fn system_writes_touch_without_warming_and_survive_database_commit() {
    for private in [false, true] {
        let mut journal = journal();
        let address = Address::repeat_byte(1);
        let key = U256::from(7);
        let value = FlaggedStorage::new(U256::from(42), private);
        journal.system_store(address, key, value).unwrap();
        let account = &journal.inner.state[&address];
        assert!(account.is_touched());
        assert!(account.is_cold_transaction_id(journal.transaction_id));
        assert!(account.storage[&key].is_cold_transaction_id(journal.transaction_id));
        journal.commit_tx();
        let state = journal.finalize();
        journal.database.commit(state);
        assert_eq!(
            journal.database.cache.accounts[&address].storage[&key],
            value
        );
    }
}

#[test]
fn discard_rolls_back_new_touches_values_and_flags_but_preserves_existing_touches() {
    for pre_touched in [false, true] {
        let mut journal = journal();
        let address = Address::repeat_byte(1);
        let key = U256::from(7);
        journal.system_storage(address, key).unwrap();
        if pre_touched {
            journal.touch_account(address);
            journal.commit_tx();
        }
        journal
            .system_store(address, key, FlaggedStorage::new(U256::from(42), true))
            .unwrap();
        journal.discard_tx();
        assert_eq!(journal.inner.state[&address].is_touched(), pre_touched);
        assert_eq!(
            journal.system_storage(address, key).unwrap().data,
            U256::ZERO
        );
        assert!(!journal.system_storage(address, key).unwrap().is_private);
    }
}

#[test]
fn system_writes_can_set_zero_while_retaining_the_registered_visibility() {
    let mut journal = journal();
    let address = Address::repeat_byte(1);
    let key = U256::from(7);
    journal
        .system_store(address, key, FlaggedStorage::new(U256::from(42), true))
        .unwrap();
    journal
        .system_store(address, key, FlaggedStorage::new(U256::ZERO, true))
        .unwrap();
    let slot = journal.system_storage(address, key).unwrap();
    assert_eq!(slot.data, U256::ZERO);
    assert!(slot.is_private);
}

#[test]
fn caller_accounting_restores_only_the_current_transaction_in_a_retained_journal() {
    for pre_touched in [false, true] {
        let mut journal = journal();
        let address = Address::repeat_byte(1);
        journal.load_account(address).unwrap();
        if pre_touched {
            journal.touch_account(address);
            journal.commit_tx();
        }
        let original_nonce = journal.inner.state[&address].info.nonce;
        journal.inner.state.get_mut(&address).unwrap().info.nonce += 1;
        journal.caller_accounting_journal_entry(address, U256::ZERO, true);
        journal.discard_tx();
        assert_eq!(journal.inner.state[&address].info.nonce, original_nonce);
        assert_eq!(journal.inner.state[&address].is_touched(), pre_touched);

        journal.inner.state.get_mut(&address).unwrap().info.nonce += 1;
        journal.inner.state.get_mut(&address).unwrap().info.balance = U256::from(10);
        journal.caller_accounting_journal_entry(address, U256::ZERO, true);
        journal.commit_tx();
        journal.inner.state.get_mut(&address).unwrap().info.nonce += 1;
        journal.inner.state.get_mut(&address).unwrap().info.balance = U256::from(20);
        journal.caller_accounting_journal_entry(address, U256::from(10), true);
        journal.discard_tx();
        assert!(journal.inner.state[&address].is_touched());
        assert_eq!(journal.inner.state[&address].info.nonce, original_nonce + 1);
        let state = journal.finalize();
        journal.database.commit(state);
        assert_eq!(
            journal.database.cache.accounts[&address].info.nonce,
            original_nonce + 1
        );
        assert_eq!(
            journal.database.cache.accounts[&address].info.balance,
            U256::from(10)
        );
    }
}
