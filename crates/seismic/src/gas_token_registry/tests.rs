#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
use super::*;
use revm::primitives::HashMap;
use std::vec::Vec;

const CALLER: Address = Address::repeat_byte(0x11);
const A: Address = Address::repeat_byte(0xaa);
const B: Address = Address::repeat_byte(0xbb);
const C: Address = Address::repeat_byte(0xcc);

#[derive(Default)]
struct Reader {
    storage: HashMap<(Address, U256), FlaggedStorage>,
    reads: Vec<(Address, U256)>,
    failure: Option<(Address, U256)>,
}

impl RegistryStorage for Reader {
    type Error = &'static str;
    fn read_storage(
        &mut self,
        address: Address,
        slot: U256,
    ) -> Result<FlaggedStorage, Self::Error> {
        self.reads.push((address, slot));
        if self.failure == Some((address, slot)) {
            return Err("provider failure");
        }
        Ok(self
            .storage
            .get(&(address, slot))
            .copied()
            .unwrap_or(FlaggedStorage::ZERO))
    }
}

impl Reader {
    #[allow(clippy::too_many_arguments)] // Explicit packed-field/balance fixtures.
    fn entry(
        &mut self,
        index: u8,
        token: Address,
        active: u8,
        mode: u8,
        decimals: u8,
        root: U256,
        balance: FlaggedStorage,
    ) {
        let packed = U256::from_be_slice(token.as_slice())
            | (U256::from(active) << 160usize)
            | (U256::from(mode) << 168usize)
            | (U256::from(decimals) << 176usize);
        self.storage.insert(
            (GAS_TOKEN_REGISTRY, TOKEN_COUNT_SLOT),
            U256::from(index + 1).into(),
        );
        self.storage.insert(
            (GAS_TOKEN_REGISTRY, token_metadata_slot(index)),
            packed.into(),
        );
        self.storage.insert(
            (
                GAS_TOKEN_REGISTRY,
                token_metadata_slot(index) + U256::from(1),
            ),
            root.into(),
        );
        self.storage
            .insert((token, balance_storage_key(CALLER, root)), balance);
    }

    fn select(
        &mut self,
        payment: GasPayment,
        native: u64,
        value: u64,
        gas: u64,
    ) -> Result<SelectedPayment, RegistryError<&'static str>> {
        select_payment(
            self,
            payment,
            CALLER,
            U256::from(native),
            U256::from(value),
            U256::from(gas),
        )
    }
}

#[test]
fn registered_token_visitor_reads_configuration_without_holder_balances() {
    let mut reader = Reader::default();
    let root = U256::MAX;
    reader.entry(0, A, 0, 255, 255, root, FlaggedStorage::ZERO);
    reader.entry(1, B, 1, 255, 6, root, FlaggedStorage::ZERO);
    reader.entry(2, C, 1, 0, 19, root, FlaggedStorage::ZERO);
    reader.entry(3, A, 1, 1, 0, root, FlaggedStorage::public(U256::from(5)));
    reader.failure = Some((A, balance_storage_key(CALLER, root)));

    let mut configurations = Vec::new();
    visit_registered_tokens(&mut reader, |token| configurations.push(token)).unwrap();
    assert_eq!(
        configurations,
        vec![GasToken {
            token: A,
            balance_slot: root,
            mode: BalanceStorageMode::Public,
            precision: TokenPrecision::new(0).unwrap(),
        }]
    );
    assert_eq!(
        reader.reads,
        vec![
            (GAS_TOKEN_REGISTRY, TOKEN_COUNT_SLOT),
            (GAS_TOKEN_REGISTRY, token_metadata_slot(0)),
            (GAS_TOKEN_REGISTRY, token_metadata_slot(1)),
            (GAS_TOKEN_REGISTRY, token_metadata_slot(2)),
            (GAS_TOKEN_REGISTRY, token_metadata_slot(3)),
            (GAS_TOKEN_REGISTRY, token_metadata_slot(3) + U256::from(1)),
        ]
    );
}

#[test]
fn registered_token_visitor_propagates_required_registry_read_errors() {
    for failed_slot in [
        TOKEN_COUNT_SLOT,
        token_metadata_slot(0),
        token_metadata_slot(0) + U256::from(1),
    ] {
        let mut reader = Reader::default();
        reader.entry(0, A, 1, 0, 18, U256::MAX, FlaggedStorage::ZERO);
        reader.failure = Some((GAS_TOKEN_REGISTRY, failed_slot));
        assert_eq!(
            visit_registered_tokens(&mut reader, |_| {}),
            Err(RegistryError::Storage("provider failure"))
        );
    }
}

#[test]
fn all_precisions_have_bounded_exact_rounding_including_maximum_u256() {
    for decimals in 0..=18 {
        let precision = TokenPrecision::new(decimals).unwrap();
        let divisor = U256::from(10u64.pow(u32::from(18 - decimals)));
        assert_eq!(precision.decimals(), decimals);
        assert_eq!(precision.divisor(), divisor);
        for amount in [
            U256::ZERO,
            U256::from(1),
            divisor - U256::from(1),
            divisor,
            divisor + U256::from(1),
            U256::MAX,
        ] {
            let wide = U512::from(amount);
            let wide_divisor = U512::from(divisor);
            assert_eq!(
                U512::from(precision.ceil(amount)),
                (wide + wide_divisor - U512::from(1)) / wide_divisor
            );
            assert_eq!(precision.floor(amount), amount / divisor);
        }
    }
    for decimals in 19..=255 {
        assert!(TokenPrecision::new(decimals).is_none());
    }
}

#[test]
fn six_decimal_conversion_matches_independent_existing_policy() {
    let precision = TokenPrecision::new(6).unwrap();
    for wei in [
        0u128,
        1,
        999_999_999_999,
        1_000_000_000_000,
        1_000_000_000_001,
        u128::MAX,
    ] {
        assert_eq!(
            precision.ceil(U256::from(wei)),
            U256::from(wei.div_ceil(1_000_000_000_000))
        );
        assert_eq!(
            precision.floor(U256::from(wei)),
            U256::from(wei / 1_000_000_000_000)
        );
    }
}

#[test]
fn allowance_divides_a_wide_product_before_capping() {
    for decimals in 0..=18 {
        let precision = TokenPrecision::new(decimals).unwrap();
        for balance in [
            U256::ZERO,
            U256::from(1),
            U256::MAX / U256::from(2),
            U256::MAX,
        ] {
            for price in [U256::from(1), U256::from(1_000_000_000u64), U256::MAX] {
                let expected = (U512::from(balance) * U512::from(precision.divisor())
                    / U512::from(price))
                .min(U512::from(u64::MAX))
                .to::<u64>();
                assert_eq!(
                    precision.gas_allowance(balance, price, u64::MAX),
                    Some(expected)
                );
            }
        }
        assert_eq!(
            precision.gas_allowance(U256::MAX, U256::ZERO, u64::MAX),
            None
        );
    }
    let precision = TokenPrecision::new(0).unwrap();
    assert_eq!(
        precision.gas_allowance(U256::MAX, U256::MAX, u64::MAX),
        Some(1_000_000_000_000_000_000)
    );
}

#[test]
fn metadata_uses_only_its_field_bytes_and_ignores_upper_padding() {
    for flag in 0..=255u8 {
        for mode in 0..=1u8 {
            for decimals in 0..=18u8 {
                let padding = U256::MAX << 184usize;
                let word = U256::from_be_slice(A.as_slice())
                    | (U256::from(flag) << 160usize)
                    | (U256::from(mode) << 168usize)
                    | (U256::from(decimals) << 176usize)
                    | padding;
                let entry = TokenMetadata::decode(word);
                assert_eq!(
                    entry,
                    TokenMetadata {
                        token: A,
                        active: flag != 0,
                        mode,
                        decimals
                    }
                );
                assert_eq!(entry.validate().is_ok(), flag != 0);
            }
        }
    }
}

#[test]
fn mapping_keys_commit_to_the_entire_256_bit_root() {
    let root = U256::MAX;
    let mut abi = Vec::new();
    abi.extend_from_slice(&[0u8; 12]);
    abi.extend_from_slice(CALLER.as_slice());
    abi.extend_from_slice(&root.to_be_bytes::<32>());
    assert_eq!(
        balance_storage_key(CALLER, root),
        U256::from_be_bytes(keccak256(&abi).0)
    );
    assert_ne!(
        balance_storage_key(CALLER, root),
        balance_storage_key(CALLER, U256::from(255))
    );
    assert_ne!(
        balance_storage_key(CALLER, root),
        balance_storage_key(A, root)
    );
}

#[test]
fn mode_eligibility_matches_value_and_privacy_matrix() {
    for value in [U256::ZERO, U256::from(1)] {
        for private in [false, true] {
            let balance = FlaggedStorage::new(value, private);
            assert_eq!(BalanceStorageMode::Public.accepts(balance), !private);
            assert_eq!(
                BalanceStorageMode::Shielded.accepts(balance),
                private || value.is_zero()
            );
        }
    }
}

#[test]
fn native_fast_paths_and_native_failure_never_read_the_registry() {
    let mut reader = Reader {
        failure: Some((GAS_TOKEN_REGISTRY, TOKEN_COUNT_SLOT)),
        ..Default::default()
    };
    assert_eq!(
        reader.select(GasPayment::Auto, 100, 10, 90),
        Ok(SelectedPayment::Native)
    );
    assert_eq!(
        reader.select(GasPayment::Native, 100, 10, 90),
        Ok(SelectedPayment::Native)
    );
    assert!(matches!(
        reader.select(GasPayment::Native, 100, 10, 91),
        Err(RegistryError::Transaction(
            InvalidTransaction::LackOfFundForMaxFee { .. }
        ))
    ));
    assert!(reader.reads.is_empty());
    assert_eq!(
        reader.select(GasPayment::Auto, 100, 10, 91),
        Err(RegistryError::Storage("provider failure"))
    );
}

#[test]
fn native_value_cannot_be_funded_or_split_with_tokens() {
    let mut reader = Reader::default();
    reader.entry(0, A, 1, 1, 18, U256::MAX, U256::from(1_000).into());
    assert!(matches!(
        reader.select(GasPayment::Token(A), 1, 2, 1),
        Err(RegistryError::Transaction(
            InvalidTransaction::LackOfFundForMaxFee { .. }
        ))
    ));
    assert!(reader.reads.is_empty());
    assert!(
        matches!(reader.select(GasPayment::Token(A), 0, 0, 1), Ok(SelectedPayment::Token(token)) if token.token == A)
    );
    reader.storage.insert(
        (A, balance_storage_key(CALLER, U256::MAX)),
        U256::from(50).into(),
    );
    reader.entry(1, B, 1, 1, 18, U256::from(7), U256::from(50).into());
    assert!(matches!(
        reader.select(GasPayment::Auto, 50, 0, 100),
        Err(RegistryError::Transaction(
            InvalidTransaction::LackOfFundForMaxFee { .. }
        ))
    ));
}

#[test]
fn auto_skips_inactive_unsupported_and_incompatible_entries_lazily() {
    for (active, mode, decimals) in [(0, 255, 255), (1, 2, 6), (1, 1, 19)] {
        let mut reader = Reader::default();
        reader.entry(0, A, active, mode, decimals, U256::MAX, U256::MAX.into());
        reader.entry(1, B, 1, 1, 18, U256::from(7), U256::from(100).into());
        reader.failure = Some((GAS_TOKEN_REGISTRY, token_metadata_slot(0) + U256::from(1)));
        assert!(
            matches!(reader.select(GasPayment::Auto, 0, 0, 100), Ok(SelectedPayment::Token(token)) if token.token == B)
        );
        assert_eq!(
            reader.reads,
            [
                (GAS_TOKEN_REGISTRY, TOKEN_COUNT_SLOT),
                (GAS_TOKEN_REGISTRY, token_metadata_slot(0)),
                (GAS_TOKEN_REGISTRY, token_metadata_slot(1)),
                (GAS_TOKEN_REGISTRY, token_metadata_slot(1) + U256::from(1)),
                (B, balance_storage_key(CALLER, U256::from(7))),
            ]
        );
    }
    let mut reader = Reader::default();
    reader.entry(0, A, 1, 0, 18, U256::MAX, U256::from(100).into());
    reader.entry(1, B, 1, 1, 18, U256::from(7), U256::from(100).into());
    assert!(
        matches!(reader.select(GasPayment::Auto, 0, 0, 100), Ok(SelectedPayment::Token(token)) if token.token == B)
    );
}

#[test]
fn exact_auto_stops_before_later_provider_failures_but_aggregate_does_not() {
    let mut reader = Reader::default();
    reader.entry(0, A, 1, 1, 18, U256::MAX, U256::from(100).into());
    reader.entry(1, B, 1, 1, 18, U256::from(7), U256::from(100).into());
    reader.failure = Some((GAS_TOKEN_REGISTRY, token_metadata_slot(1)));
    assert!(
        matches!(reader.select(GasPayment::Auto, 0, 0, 100), Ok(SelectedPayment::Token(token)) if token.token == A)
    );
    assert!(!reader
        .reads
        .contains(&(GAS_TOKEN_REGISTRY, token_metadata_slot(1))));
    assert_eq!(
        aggregate_balance(&mut reader, CALLER, U256::ZERO),
        Err(RegistryError::Storage("provider failure"))
    );
}

#[test]
fn explicit_lookup_reads_only_metadata_until_the_match_then_selected_balance() {
    let mut reader = Reader::default();
    reader.entry(0, A, 0, 255, 255, U256::MAX, U256::MAX.into());
    reader.entry(1, B, 1, 1, 8, U256::from(7), U256::from(100).into());
    reader.entry(2, C, 1, 1, 18, U256::from(9), U256::from(100).into());
    reader.failure = Some((GAS_TOKEN_REGISTRY, token_metadata_slot(2)));
    assert!(
        matches!(reader.select(GasPayment::Token(B), 1_000, 0, 100), Ok(SelectedPayment::Token(token)) if token.token == B && token.precision.decimals() == 8)
    );
    assert_eq!(
        reader.reads,
        [
            (GAS_TOKEN_REGISTRY, TOKEN_COUNT_SLOT),
            (GAS_TOKEN_REGISTRY, token_metadata_slot(0)),
            (GAS_TOKEN_REGISTRY, token_metadata_slot(1)),
            (GAS_TOKEN_REGISTRY, token_metadata_slot(1) + U256::from(1)),
            (B, balance_storage_key(CALLER, U256::from(7))),
        ]
    );
}

#[test]
fn explicit_ineligible_metadata_fails_without_root_reads_or_fallback() {
    for (active, mode, decimals, expected) in [
        (0, 255, 255, InvalidTransaction::GasTokenInactive(A)),
        (
            1,
            2,
            6,
            InvalidTransaction::UnsupportedGasTokenMode { token: A, mode: 2 },
        ),
        (
            1,
            1,
            19,
            InvalidTransaction::UnsupportedGasTokenDecimals {
                token: A,
                decimals: 19,
            },
        ),
    ] {
        let mut reader = Reader::default();
        reader.entry(0, A, active, mode, decimals, U256::MAX, U256::MAX.into());
        reader.entry(1, B, 1, 1, 18, U256::from(7), U256::MAX.into());
        reader.failure = Some((GAS_TOKEN_REGISTRY, token_metadata_slot(0) + U256::from(1)));
        assert_eq!(
            reader.select(GasPayment::Token(A), 1_000, 0, 100),
            Err(RegistryError::Transaction(expected))
        );
        assert_eq!(reader.reads.len(), 2);
    }
}

#[test]
fn explicit_unknown_scans_only_metadata_and_zero_is_invalid() {
    let mut reader = Reader::default();
    reader.entry(0, A, 1, 1, 18, U256::MAX, U256::MAX.into());
    reader.entry(1, B, 1, 1, 18, U256::from(7), U256::MAX.into());
    assert_eq!(
        reader.select(GasPayment::Token(C), 1_000, 0, 100),
        Err(RegistryError::Transaction(
            InvalidTransaction::GasTokenNotRegistered(C)
        ))
    );
    assert_eq!(reader.reads.len(), 3);
    reader.reads.clear();
    assert_eq!(
        reader.select(GasPayment::Token(Address::ZERO), 1_000, 0, 100),
        Err(RegistryError::Transaction(
            InvalidTransaction::InvalidGasPaymentSelector
        ))
    );
    assert!(reader.reads.is_empty());
}

#[test]
fn selected_mode_mismatch_is_typed_and_never_converts_the_slot() {
    let mut reader = Reader::default();
    reader.entry(0, A, 1, 0, 18, U256::MAX, U256::from(100).into());
    let before = reader.storage.clone();
    assert_eq!(
        reader.select(GasPayment::Token(A), 1_000, 0, 100),
        Err(RegistryError::Transaction(
            InvalidTransaction::GasTokenBalanceModeMismatch {
                token: A,
                account: CALLER
            }
        ))
    );
    assert_eq!(reader.storage, before);
    reader.storage.insert(
        (A, balance_storage_key(CALLER, U256::MAX)),
        FlaggedStorage::ZERO,
    );
    assert!(matches!(
        reader.select(GasPayment::Token(A), 0, 0, 0),
        Ok(SelectedPayment::Token(_))
    ));
    assert_eq!(
        reader.storage[&(A, balance_storage_key(CALLER, U256::MAX))],
        FlaggedStorage::ZERO
    );
}

#[test]
fn required_read_failures_are_not_skipped_and_oversized_counts_are_invalid() {
    for fail in [
        (GAS_TOKEN_REGISTRY, TOKEN_COUNT_SLOT),
        (GAS_TOKEN_REGISTRY, token_metadata_slot(0)),
        (GAS_TOKEN_REGISTRY, token_metadata_slot(0) + U256::from(1)),
        (A, balance_storage_key(CALLER, U256::MAX)),
    ] {
        let mut reader = Reader::default();
        reader.entry(0, A, 1, 1, 18, U256::MAX, U256::MAX.into());
        reader.entry(1, B, 1, 1, 18, U256::from(7), U256::MAX.into());
        reader.failure = Some(fail);
        assert_eq!(
            reader.select(GasPayment::Auto, 0, 0, 100),
            Err(RegistryError::Storage("provider failure"))
        );
        assert_eq!(
            reader.select(GasPayment::Token(A), 1_000, 0, 100),
            Err(RegistryError::Storage("provider failure"))
        );
    }
    let mut reader = Reader::default();
    reader.storage.insert(
        (GAS_TOKEN_REGISTRY, TOKEN_COUNT_SLOT),
        U256::from(33).into(),
    );
    assert_eq!(
        reader.select(GasPayment::Auto, 0, 0, 100),
        Err(RegistryError::Transaction(
            InvalidTransaction::GasTokenRegistryTooLarge
        ))
    );
    assert_eq!(
        reader.select(GasPayment::Token(A), 1_000, 0, 100),
        Err(RegistryError::Transaction(
            InvalidTransaction::GasTokenRegistryTooLarge
        ))
    );
}

#[test]
fn mixed_decimal_aggregate_sums_eligible_balances_without_authorizing_splitting() {
    let mut reader = Reader::default();
    reader.entry(0, A, 1, 1, 6, U256::MAX, U256::from(10).into());
    reader.entry(1, B, 1, 1, 8, U256::from(7), U256::from(10).into());
    reader.entry(2, C, 1, 0, 18, U256::from(9), U256::MAX.into()); // nonzero public contradicts Shielded
    assert_eq!(
        aggregate_balance(&mut reader, CALLER, U256::from(7)).unwrap(),
        U256::from(10_100_000_000_007u64)
    );
    assert!(matches!(
        reader.select(GasPayment::Auto, 0, 0, 10_100_000_000_000),
        Err(RegistryError::Transaction(
            InvalidTransaction::LackOfFundForMaxFee { .. }
        ))
    ));
    reader.storage.insert(
        (A, balance_storage_key(CALLER, U256::MAX)),
        U256::MAX.into(),
    );
    assert_eq!(
        aggregate_balance(&mut reader, CALLER, U256::from(7)).unwrap(),
        U256::MAX
    );
}

#[test]
fn runtime_configuration_reads_ignore_private_flags_and_observe_fresh_metadata() {
    let mut reader = Reader::default();
    reader.entry(0, A, 1, 1, 18, U256::MAX, U256::from(100).into());
    for ((address, _), value) in reader.storage.iter_mut() {
        if *address == GAS_TOKEN_REGISTRY {
            value.is_private = true;
        }
    }
    assert!(matches!(
        reader.select(GasPayment::Auto, 0, 0, 100),
        Ok(SelectedPayment::Token(_))
    ));
    reader
        .storage
        .get_mut(&(GAS_TOKEN_REGISTRY, token_metadata_slot(0)))
        .unwrap()
        .value &= !(U256::from(0xff) << 160usize);
    assert_eq!(
        reader.select(GasPayment::Token(A), 1_000, 0, 100),
        Err(RegistryError::Transaction(
            InvalidTransaction::GasTokenInactive(A)
        ))
    );
}
