use super::entry::JournalEntry;
use super::inner::JournalInner;
use database::InMemoryDB;
use database_interface::Database;
use primitives::{hardfork::SpecId, Address, FlaggedStorage, U256};
use state::{Account, AccountInfo, AccountStatus, EvmStorage};

/// Test that demonstrates the database correctly preserves privacy flags
#[test]
fn test_database_privacy_flag_preservation() {
    let mut db = InMemoryDB::default();
    let address = Address::from_slice(&[0x2; 20]);
    let storage_key = U256::from(1);

    // Store private data directly in database
    let private_storage = FlaggedStorage::from(U256::from(42)).mark_private();
    db.insert_account_storage(address, storage_key, private_storage)
        .unwrap();

    // Verify database preserves privacy flag
    let db_result = db.storage(address, storage_key).unwrap();
    assert!(
        db_result.is_private,
        "Database should preserve private flag"
    );
    assert_eq!(
        db_result.value,
        U256::from(42),
        "Database should preserve value"
    );

    // Compare with what journal ZERO returns
    let journal_zero = FlaggedStorage::ZERO.set_visibility(false);
    assert!(!journal_zero.is_private, "Journal ZERO should be public");
    assert_eq!(
        journal_zero.value,
        U256::ZERO,
        "Journal ZERO should be zero value"
    );

    // This demonstrates the inconsistency: same storage key returns different privacy flags
    println!(
        "Database storage: value={}, is_private={}",
        db_result.value, db_result.is_private
    );
    println!(
        "Journal ZERO: value={}, is_private={}",
        journal_zero.value, journal_zero.is_private
    );

    // Show that they are indeed different
    assert_ne!(
        db_result.is_private, journal_zero.is_private,
        "Database and journal ZERO should have different privacy flags"
    );
}

/// Test demonstrating the core problem: sload returns different values
/// based on account creation status for the same storage key
#[test]
fn test_sload_inconsistency_demonstration() {
    let mut db = InMemoryDB::default();
    let address = Address::from_slice(&[0x1; 20]);
    let storage_key = U256::from(0);

    // Setup: Pre-populate database with some storage value
    // In real scenario, this would be marked private via CSTORE
    let stored_value = FlaggedStorage::from(U256::from(123)).mark_private();
    db.insert_account_storage(address, storage_key, stored_value)
        .unwrap();

    // Verify what the database actually contains
    let db_stored = db.storage(address, storage_key).unwrap();
    println!(
        "Database contains: value={}, is_private={}",
        db_stored.value, db_stored.is_private
    );

    // Test 1: Account with Created status (newly created)
    let mut journal1 = JournalInner::<JournalEntry>::new();
    journal1.spec = SpecId::MERCURY;

    let account_info = AccountInfo {
        nonce: 1,
        balance: U256::from(1000),
        ..Default::default()
    };
    journal1.state.insert(
        address,
        Account {
            info: account_info.clone(),
            status: AccountStatus::Created, // This makes is_newly_created = true
            storage: EvmStorage::new(),
        },
    );

    // Note: sload now returns StateLoad<FlaggedStorage> - privacy information is preserved!
    let result1 = journal1.sload(&mut db, address, storage_key).unwrap();
    println!(
        "Newly created account sload result: value={}, is_private={}",
        result1.data.value, result1.data.is_private
    );

    // Test 2: Account with Loaded status (not newly created)
    let mut journal2 = JournalInner::<JournalEntry>::new();
    journal2.spec = SpecId::MERCURY;

    journal2.state.insert(
        address,
        Account {
            info: account_info,
            status: AccountStatus::Loaded, // This makes is_newly_created = false
            storage: EvmStorage::new(),
        },
    );

    let result2 = journal2.sload(&mut db, address, storage_key).unwrap();
    println!(
        "Existing account sload result: value={}, is_private={}",
        result2.data.value, result2.data.is_private
    );

    // The issue: These should be the same since they're accessing the same storage
    // but they might not be due to the logic in sload that uses is_newly_created
    println!("Values are equal: {}", result1.data == result2.data);
    println!(
        "Privacy flags equal: {}",
        result1.data.is_private == result2.data.is_private
    );

    // This test exposes the bug - both the values AND privacy flags should be identical
    assert_eq!(result1.data, result2.data,
               "Same storage slot should return identical FlaggedStorage regardless of account creation status");

    // Additional specific check for the privacy flag inconsistency
    assert_eq!(
        result1.data.is_private, result2.data.is_private,
        "Privacy flags should be consistent! Got newly_created={} vs not_newly_created={}",
        result1.data.is_private, result2.data.is_private
    );
}

/// Test private storage behavior during simple revert scenarios
#[test]
fn test_private_storage_simple_revert() {
    let mut db = InMemoryDB::default();
    let address = Address::from_slice(&[0x1; 20]);
    let storage_key = U256::from(1);

    let mut journal = JournalInner::<JournalEntry>::new();
    journal.spec = SpecId::MERCURY;

    // Setup account
    let account_info = AccountInfo {
        nonce: 1,
        balance: U256::from(1000),
        ..Default::default()
    };
    journal.state.insert(
        address,
        Account {
            info: account_info,
            status: AccountStatus::Loaded,
            storage: EvmStorage::new(),
        },
    );

    // Create checkpoint before storing private data
    let checkpoint = journal.checkpoint();

    // Store private data using CSTORE
    let cstore_result = journal
        .cstore(&mut db, address, storage_key, U256::from(42))
        .unwrap();
    println!(
        "Stored private data: value={}, is_private={}",
        cstore_result.data.new_value.value, cstore_result.data.new_value.is_private
    );

    // Verify storage was marked as private
    assert!(
        cstore_result.data.new_value.is_private,
        "CSTORE should mark storage as private"
    );

    // Check current state before revert
    let before_revert = journal.sload(&mut db, address, storage_key).unwrap();
    println!(
        "Before revert sload: value={}, is_private={}",
        before_revert.data.value, before_revert.data.is_private
    );

    // Revert to checkpoint
    journal.checkpoint_revert(checkpoint);

    // Check state after revert - should be back to original (zero/empty)
    let after_revert = journal.sload(&mut db, address, storage_key).unwrap();
    println!(
        "After revert sload: value={}, is_private={}",
        after_revert.data.value, after_revert.data.is_private
    );

    // The key test: revert should properly restore the previous state
    assert_eq!(
        after_revert.data.value,
        U256::ZERO,
        "Storage value should be zero after reverting private storage write"
    );
    assert!(
        !after_revert.data.is_private,
        "Storage should be public (not private) after revert to empty state"
    );
}

/// Test mixed private/public storage reverts to ensure privacy flags are handled correctly
#[test]
fn test_mixed_storage_revert() {
    let mut db = InMemoryDB::default();
    let address = Address::from_slice(&[0x2; 20]);
    let private_key = U256::from(1);
    let public_key = U256::from(2);

    let mut journal = JournalInner::<JournalEntry>::new();
    journal.spec = SpecId::MERCURY;

    // Setup account
    journal.state.insert(
        address,
        Account {
            info: AccountInfo {
                nonce: 1,
                balance: U256::from(1000),
                ..Default::default()
            },
            status: AccountStatus::Loaded,
            storage: EvmStorage::new(),
        },
    );

    // Store initial public data
    let initial_public_store = journal
        .sstore(&mut db, address, public_key, U256::from(100))
        .unwrap();
    println!(
        "Initial public store: is_private={}",
        initial_public_store.data.new_value.is_private
    );

    // Create checkpoint
    let checkpoint = journal.checkpoint();

    // Store private data
    let private_store = journal
        .cstore(&mut db, address, private_key, U256::from(200))
        .unwrap();
    println!(
        "Private store: is_private={}",
        private_store.data.new_value.is_private
    );
    assert!(
        private_store.data.new_value.is_private,
        "CSTORE should be private"
    );

    // Modify existing public data
    let public_modify = journal
        .sstore(&mut db, address, public_key, U256::from(300))
        .unwrap();
    println!(
        "Public modify: is_private={}",
        public_modify.data.new_value.is_private
    );
    assert!(
        !public_modify.data.new_value.is_private,
        "SSTORE should remain public"
    );

    // Verify both storages exist with correct values and privacy flags
    let private_read = journal.sload(&mut db, address, private_key).unwrap();
    let public_read = journal.sload(&mut db, address, public_key).unwrap();
    println!(
        "Before revert - private: {} (private={}), public: {} (private={})",
        private_read.data.value,
        private_read.data.is_private,
        public_read.data.value,
        public_read.data.is_private
    );

    // Revert to checkpoint
    journal.checkpoint_revert(checkpoint);

    // After revert: private should be gone (zero), public should be restored to 100
    let private_after = journal.sload(&mut db, address, private_key).unwrap();
    let public_after = journal.sload(&mut db, address, public_key).unwrap();
    println!(
        "After revert - private: {} (private={}), public: {} (private={})",
        private_after.data.value,
        private_after.data.is_private,
        public_after.data.value,
        public_after.data.is_private
    );

    assert_eq!(
        private_after.data.value,
        U256::ZERO,
        "Private storage value should be reverted to zero"
    );
    assert!(
        !private_after.data.is_private,
        "Reverted private storage should be public (empty)"
    );
    assert_eq!(
        public_after.data.value,
        U256::from(100),
        "Public storage value should be reverted to original"
    );
    assert!(
        !public_after.data.is_private,
        "Public storage should remain public after revert"
    );
}

/// Test account creation with private storage followed by revert
#[test]
fn test_account_creation_private_storage_revert() {
    let mut db = InMemoryDB::default();
    let address = Address::from_slice(&[0x3; 20]);
    let storage_key = U256::from(1);

    let mut journal = JournalInner::<JournalEntry>::new();
    journal.spec = SpecId::MERCURY;

    // Create checkpoint before account creation
    let checkpoint = journal.checkpoint();

    // Create new account
    journal.load_account(&mut db, address).unwrap();
    let account = journal.state.get_mut(&address).unwrap();
    account.mark_created(); // Mark as newly created
    account.mark_touch(); // Mark as touched
    account.info = AccountInfo {
        nonce: 1,
        balance: U256::from(1000),
        ..Default::default()
    };

    println!("Account created, is_created: {}", account.is_created());

    // Store private data in the newly created account
    let store_result = journal
        .cstore(&mut db, address, storage_key, U256::from(99))
        .unwrap();
    println!(
        "Stored in new account: is_private={}",
        store_result.data.new_value.is_private
    );
    assert!(
        store_result.data.new_value.is_private,
        "CSTORE in new account should be private"
    );

    // Verify storage exists
    let read_result = journal.sload(&mut db, address, storage_key).unwrap();
    println!(
        "Read from new account: value={}, is_private={}",
        read_result.data.value, read_result.data.is_private
    );
    assert_eq!(
        read_result.data.value,
        U256::from(99),
        "Should read stored value"
    );
    assert!(
        read_result.data.is_private,
        "Stored value should be private"
    );

    // Revert account creation
    journal.checkpoint_revert(checkpoint);

    // After revert: account should not exist, so sload should return zero
    // But more importantly, the account creation flag should be properly reverted
    if let Some(account) = journal.state.get(&address) {
        println!("Account after revert, is_created: {}", account.is_created());
        assert!(
            !account.is_created(),
            "Account should not be marked as created after revert"
        );

        let read_after_revert = journal.sload(&mut db, address, storage_key).unwrap();
        println!(
            "Read after revert: value={}, is_private={}",
            read_after_revert.data.value, read_after_revert.data.is_private
        );
        // This tests the core bug - what value do we get after reverting account creation?
        assert_eq!(
            read_after_revert.data.value,
            U256::ZERO,
            "Storage should be zero after account creation revert"
        );
    } else {
        println!("Account does not exist after revert (expected)");
        // If account doesn't exist, we can't test sload, but this is also valid behavior
    }
}

/// Test nested checkpoint reverts with private storage
#[test]
fn test_nested_checkpoint_private_storage_reverts() {
    let mut db = InMemoryDB::default();
    let address = Address::from_slice(&[0x4; 20]);
    let key1 = U256::from(1);
    let key2 = U256::from(2);

    let mut journal = JournalInner::<JournalEntry>::new();
    journal.spec = SpecId::MERCURY;

    // Setup account
    journal.state.insert(
        address,
        Account {
            info: AccountInfo {
                nonce: 1,
                balance: U256::from(1000),
                ..Default::default()
            },
            status: AccountStatus::Loaded,
            storage: EvmStorage::new(),
        },
    );

    // Level 0: Store initial value
    journal
        .cstore(&mut db, address, key1, U256::from(10))
        .unwrap();

    // Level 1: Create checkpoint and store more data
    let checkpoint1 = journal.checkpoint();
    journal
        .cstore(&mut db, address, key1, U256::from(20))
        .unwrap(); // Modify existing
    journal
        .cstore(&mut db, address, key2, U256::from(30))
        .unwrap(); // New key

    // Level 2: Create another checkpoint and store even more data
    let checkpoint2 = journal.checkpoint();
    journal
        .cstore(&mut db, address, key1, U256::from(40))
        .unwrap(); // Modify again

    // Verify current state
    let read1 = journal.sload(&mut db, address, key1).unwrap();
    let read2 = journal.sload(&mut db, address, key2).unwrap();
    println!(
        "Before any revert - key1: {} (private={}), key2: {} (private={})",
        read1.data.value, read1.data.is_private, read2.data.value, read2.data.is_private
    );
    assert_eq!(read1.data.value, U256::from(40));
    assert_eq!(read2.data.value, U256::from(30));
    assert!(read1.data.is_private, "key1 should be private");
    assert!(read2.data.is_private, "key2 should be private");

    // Revert level 2 (should restore key1 to 20)
    journal.checkpoint_revert(checkpoint2);
    let read1_after_2 = journal.sload(&mut db, address, key1).unwrap();
    let read2_after_2 = journal.sload(&mut db, address, key2).unwrap();
    println!(
        "After level 2 revert - key1: {} (private={}), key2: {} (private={})",
        read1_after_2.data.value,
        read1_after_2.data.is_private,
        read2_after_2.data.value,
        read2_after_2.data.is_private
    );
    assert_eq!(
        read1_after_2.data.value,
        U256::from(20),
        "key1 should revert to 20"
    );
    assert_eq!(
        read2_after_2.data.value,
        U256::from(30),
        "key2 should remain 30"
    );
    assert!(
        read1_after_2.data.is_private,
        "key1 should remain private after level 2 revert"
    );
    assert!(
        read2_after_2.data.is_private,
        "key2 should remain private after level 2 revert"
    );

    // Revert level 1 (should restore key1 to 10, key2 to 0)
    journal.checkpoint_revert(checkpoint1);
    let read1_after_1 = journal.sload(&mut db, address, key1).unwrap();
    let read2_after_1 = journal.sload(&mut db, address, key2).unwrap();
    println!(
        "After level 1 revert - key1: {} (private={}), key2: {} (private={})",
        read1_after_1.data.value,
        read1_after_1.data.is_private,
        read2_after_1.data.value,
        read2_after_1.data.is_private
    );
    assert_eq!(
        read1_after_1.data.value,
        U256::from(10),
        "key1 should revert to 10"
    );
    assert_eq!(
        read2_after_1.data.value,
        U256::ZERO,
        "key2 should revert to 0"
    );
    assert!(
        read1_after_1.data.is_private,
        "key1 should remain private after level 1 revert"
    );
    assert!(
        !read2_after_1.data.is_private,
        "key2 should be public (empty) after level 1 revert"
    );
}
