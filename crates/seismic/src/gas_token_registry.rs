//! Shared registry decoding, payment selection, and integer-only fee conversion.
//!
//! Readers supply flagged storage from a single journal/provider snapshot. Exact
//! selection is lazy; aggregate reporting and automatic RPC allowance scan every
//! eligible candidate. Provider errors are never treated as missing balances.
use revm::{
    context::result::InvalidTransaction,
    primitives::{address, keccak256, ruint::aliases::U512, Address, FlaggedStorage, U256},
};
use std::boxed::Box;

/// Fresh-chain GasTokenRegistry predeploy (ASCII `GasTokens`).
pub const GAS_TOKEN_REGISTRY: Address = address!("0000000000000000000000476173546f6b656e73");
/// Array length slot; entries have a two-word stride starting at its Keccak hash.
pub const TOKEN_COUNT_SLOT: U256 = U256::from_limbs([1, 0, 0, 0]);
/// Maximum count, including inactive entries.
pub const MAX_TOKENS: u8 = 32;
/// Token precision is bounded to the native currency's 18 decimal places.
pub const MAX_DECIMALS: u8 = 18;

/// Public, signed payment semantics. Standard transactions map to Auto.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GasPayment {
    /// Native first, then eligible registry entries in insertion order.
    #[default]
    Auto,
    /// Native currency only.
    Native,
    /// Exactly this registered token, with no fallback.
    Token(Address),
}

/// Registered balance-write semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BalanceStorageMode {
    /// Private slots, or zero-public slots eligible for private initialization.
    Shielded,
    /// Public slots only.
    Public,
}

impl BalanceStorageMode {
    /// Test both the actual value and privacy flag, without initializing a slot.
    pub fn accepts(self, balance: FlaggedStorage) -> bool {
        match self {
            Self::Public => !balance.is_private,
            Self::Shielded => balance.is_private || balance.value.is_zero(),
        }
    }

    /// Flag to apply to a nonzero balance operation's resulting value.
    pub const fn is_private(self) -> bool {
        matches!(self, Self::Shielded)
    }
}

/// Validated precision and nonzero divisor retained for all fee settlement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TokenPrecision {
    decimals: u8,
    divisor: U256,
}

impl TokenPrecision {
    /// Construct only for supported decimals; validate before bounded exponentiation.
    pub fn new(decimals: u8) -> Option<Self> {
        if decimals > MAX_DECIMALS {
            return None;
        }
        Some(Self {
            decimals,
            divisor: U256::from(10u64.pow(u32::from(MAX_DECIMALS - decimals))),
        })
    }

    /// Owner-registered decimal precision; zero is a real precision, not a default.
    pub const fn decimals(self) -> u8 {
        self.decimals
    }

    /// Base-unit scaling from raw token amounts to native wei amounts.
    pub const fn divisor(self) -> U256 {
        self.divisor
    }

    /// Ceiling requirement/debit without overflowing `amount + divisor - 1`.
    pub fn ceil(self, wei: U256) -> U256 {
        let quotient = wei / self.divisor;
        // If the remainder is nonzero, divisor >= 2 and quotient < U256::MAX.
        quotient + U256::from(u8::from(wei % self.divisor != U256::ZERO))
    }

    /// Floor-rounded caller reimbursement in raw token units.
    pub fn floor(self, wei: U256) -> U256 {
        wei / self.divisor
    }

    /// Approximate pool scalar contribution; saturation is intentional only here.
    pub fn aggregate(self, raw_balance: U256) -> U256 {
        raw_balance.saturating_mul(self.divisor)
    }

    /// Exact `floor(raw_balance * divisor / gas_price)`, capped after division.
    /// Returns None for zero gas price so the estimator retains its existing flow.
    pub fn gas_allowance(self, raw_balance: U256, gas_price: U256, cap: u64) -> Option<u64> {
        if gas_price.is_zero() {
            return None;
        }
        // A 256-bit balance times at most 10^18 fits well within 512 bits.
        let capacity = U512::from(raw_balance) * U512::from(self.divisor) / U512::from(gas_price);
        Some(capacity.min(U512::from(cap)).to::<u64>())
    }
}

/// Raw packed metadata. Decoding does not validate fields of inactive/unselected entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenMetadata {
    /// Registered proxy/token address in bits 0–159.
    pub token: Address,
    /// Only byte 20 determines activation; every nonzero value is true.
    pub active: bool,
    /// Raw storage mode at byte 21.
    pub mode: u8,
    /// Raw precision at byte 22; only bits 184–255 are padding.
    pub decimals: u8,
}

impl TokenMetadata {
    /// Decode public configuration values; runtime ignores registry privacy flags.
    pub fn decode(word: U256) -> Self {
        let mask = U256::from(0xff);
        Self {
            token: Address::from_word(word.to_be_bytes::<32>().into()),
            active: ((word >> 160usize) & mask) != U256::ZERO,
            mode: ((word >> 168usize) & mask).to::<u8>(),
            decimals: ((word >> 176usize) & mask).to::<u8>(),
        }
    }

    /// Validate a selected entry before reading its root or token balance.
    pub fn validate(self) -> Result<(BalanceStorageMode, TokenPrecision), InvalidTransaction> {
        if !self.active {
            return Err(InvalidTransaction::GasTokenInactive(self.token));
        }
        let mode = match self.mode {
            0 => BalanceStorageMode::Shielded,
            1 => BalanceStorageMode::Public,
            mode => {
                return Err(InvalidTransaction::UnsupportedGasTokenMode {
                    token: self.token,
                    mode,
                })
            }
        };
        let precision = TokenPrecision::new(self.decimals).ok_or(
            InvalidTransaction::UnsupportedGasTokenDecimals {
                token: self.token,
                decimals: self.decimals,
            },
        )?;
        Ok((mode, precision))
    }
}

/// Immutable selected configuration, retained even for a zero fee reserve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GasToken {
    /// Registered token/proxy address.
    pub token: Address,
    /// Full-width mapping root position, never truncated to one byte.
    pub balance_slot: U256,
    /// Validated write mode.
    pub mode: BalanceStorageMode,
    /// Retained registered decimals and divisor.
    pub precision: TokenPrecision,
}

/// Actual payment asset after exact maximum-fee affordability validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedPayment {
    /// No registry reads are needed for native selection.
    Native,
    /// One registered token covers the entire maximum gas cost.
    Token(GasToken),
}

/// Storage reader for one current journal/provider snapshot.
pub trait RegistryStorage {
    /// Underlying database/provider error, kept distinct from transaction invalidity.
    type Error;

    /// An absent slot is a successful zero-public read; failed reads must return Err.
    fn read_storage(&mut self, address: Address, key: U256) -> Result<FlaggedStorage, Self::Error>;
}

/// Required provider failures and deterministic invalidity use separate channels.
#[derive(Debug, PartialEq, Eq)]
pub enum RegistryError<E> {
    /// Required storage/provider read failed.
    Storage(E),
    /// Successfully read state deterministically invalidates payment.
    Transaction(InvalidTransaction),
}

/// Metadata word slot for an index already bounded by the validated array count.
pub fn token_metadata_slot(index: u8) -> U256 {
    let base = U256::from_be_bytes(keccak256(TOKEN_COUNT_SLOT.to_be_bytes::<32>()).0);
    base.wrapping_add(U256::from(u16::from(index) * 2))
}

/// Solidity mapping key `keccak256(abi.encode(account, full_width_root))`.
pub fn balance_storage_key(account: Address, balance_slot: U256) -> U256 {
    let mut encoded = [0u8; 64];
    // Fixed-size ABI words: the first word left-pads an address; the second is U256.
    #[allow(clippy::indexing_slicing)]
    {
        encoded[12..32].copy_from_slice(account.as_slice());
        encoded[32..64].copy_from_slice(&balance_slot.to_be_bytes::<32>());
    }
    keccak256(encoded).into()
}

/// Read and bound the count before any metadata/balance traversal.
pub fn token_count<R: RegistryStorage>(reader: &mut R) -> Result<u8, RegistryError<R::Error>> {
    let count = reader
        .read_storage(GAS_TOKEN_REGISTRY, TOKEN_COUNT_SLOT)
        .map_err(RegistryError::Storage)?
        .value;
    if count > U256::from(MAX_TOKENS) {
        return Err(RegistryError::Transaction(
            InvalidTransaction::GasTokenRegistryTooLarge,
        ));
    }
    Ok(count.to::<u8>())
}

fn metadata<R: RegistryStorage>(
    reader: &mut R,
    index: u8,
) -> Result<TokenMetadata, RegistryError<R::Error>> {
    reader
        .read_storage(GAS_TOKEN_REGISTRY, token_metadata_slot(index))
        .map(|word| TokenMetadata::decode(word.value))
        .map_err(RegistryError::Storage)
}

fn configuration<R: RegistryStorage>(
    reader: &mut R,
    index: u8,
    entry: TokenMetadata,
    mode: BalanceStorageMode,
    precision: TokenPrecision,
) -> Result<GasToken, RegistryError<R::Error>> {
    let root = reader
        .read_storage(
            GAS_TOKEN_REGISTRY,
            token_metadata_slot(index).wrapping_add(U256::from(1)),
        )
        .map_err(RegistryError::Storage)?
        .value;
    Ok(GasToken {
        token: entry.token,
        balance_slot: root,
        mode,
        precision,
    })
}

/// Exact explicit lookup. Nonselected entries require metadata reads only.
pub fn lookup_token<R: RegistryStorage>(
    reader: &mut R,
    token: Address,
) -> Result<GasToken, RegistryError<R::Error>> {
    if token == Address::ZERO {
        return Err(RegistryError::Transaction(
            InvalidTransaction::InvalidGasPaymentSelector,
        ));
    }
    for index in 0..token_count(reader)? {
        let entry = metadata(reader, index)?;
        if entry.token == token {
            let (mode, precision) = entry.validate().map_err(RegistryError::Transaction)?;
            return configuration(reader, index, entry, mode, precision);
        }
    }
    Err(RegistryError::Transaction(
        InvalidTransaction::GasTokenNotRegistered(token),
    ))
}

/// Read a candidate balance, preserving the privacy flag and required read errors.
pub fn token_balance<R: RegistryStorage>(
    reader: &mut R,
    token: GasToken,
    account: Address,
) -> Result<FlaggedStorage, RegistryError<R::Error>> {
    reader
        .read_storage(
            token.token,
            balance_storage_key(account, token.balance_slot),
        )
        .map_err(RegistryError::Storage)
}

fn insufficient(fee: U256, balance: U256) -> InvalidTransaction {
    InvalidTransaction::LackOfFundForMaxFee {
        fee: Box::new(fee),
        balance: Box::new(balance),
    }
}

/// Select one fee asset; value is always funded natively. Execution passes the
/// actual environment value, including zero only after known decryption failure.
pub fn select_payment<R: RegistryStorage>(
    reader: &mut R,
    selector: GasPayment,
    caller: Address,
    native_balance: U256,
    value: U256,
    maximum_gas_wei: U256,
) -> Result<SelectedPayment, RegistryError<R::Error>> {
    if value > native_balance {
        return Err(RegistryError::Transaction(insufficient(
            value,
            native_balance,
        )));
    }
    let native_requirement =
        value
            .checked_add(maximum_gas_wei)
            .ok_or(RegistryError::Transaction(
                InvalidTransaction::OverflowPaymentInTransaction,
            ))?;
    if selector != GasPayment::Token(Address::ZERO) {
        match selector {
            GasPayment::Native => {
                if native_balance < native_requirement {
                    return Err(RegistryError::Transaction(insufficient(
                        native_requirement,
                        native_balance,
                    )));
                }
                return Ok(SelectedPayment::Native);
            }
            GasPayment::Auto if native_balance >= native_requirement => {
                return Ok(SelectedPayment::Native)
            }
            _ => (),
        }
    }
    if let GasPayment::Token(address) = selector {
        let token = lookup_token(reader, address)?;
        let balance = token_balance(reader, token, caller)?;
        if !token.mode.accepts(balance) {
            return Err(RegistryError::Transaction(
                InvalidTransaction::GasTokenBalanceModeMismatch {
                    token: token.token,
                    account: caller,
                },
            ));
        }
        let required = token.precision.ceil(maximum_gas_wei);
        if balance.value < required {
            return Err(RegistryError::Transaction(insufficient(
                required,
                balance.value,
            )));
        }
        return Ok(SelectedPayment::Token(token));
    }
    for index in 0..token_count(reader)? {
        let entry = metadata(reader, index)?;
        // Inactive/unsupported metadata requires no mapping-root or token reads.
        let Ok((mode, precision)) = entry.validate() else {
            continue;
        };
        let token = configuration(reader, index, entry, mode, precision)?;
        let balance = token_balance(reader, token, caller)?;
        if mode.accepts(balance) && balance.value >= precision.ceil(maximum_gas_wei) {
            return Ok(SelectedPayment::Token(token));
        }
    }
    Err(RegistryError::Transaction(insufficient(
        native_requirement,
        native_balance,
    )))
}

/// Visit active entries with supported mode and precision without reading holder balances.
///
/// Pool maintenance uses these configurations to match changed storage keys to known
/// pooled senders. Required registry reads propagate errors; inactive or unsupported
/// entries require no mapping-root reads. This is not exact payment selection.
pub fn visit_registered_tokens<R: RegistryStorage>(
    reader: &mut R,
    mut visit: impl FnMut(GasToken),
) -> Result<(), RegistryError<R::Error>> {
    for index in 0..token_count(reader)? {
        let entry = metadata(reader, index)?;
        let Ok((mode, precision)) = entry.validate() else {
            continue;
        };
        visit(configuration(reader, index, entry, mode, precision)?);
    }
    Ok(())
}

/// Visit all active, supported, mode-compatible balances for aggregate reporting
/// or Auto RPC allowance. Unlike exact selection, this has a full read frontier.
pub fn visit_eligible_tokens<R: RegistryStorage>(
    reader: &mut R,
    caller: Address,
    mut visit: impl FnMut(GasToken, U256),
) -> Result<(), RegistryError<R::Error>> {
    for index in 0..token_count(reader)? {
        let entry = metadata(reader, index)?;
        let Ok((mode, precision)) = entry.validate() else {
            continue;
        };
        let token = configuration(reader, index, entry, mode, precision)?;
        let balance = token_balance(reader, token, caller)?;
        if mode.accepts(balance) {
            visit(token, balance.value);
        }
    }
    Ok(())
}

/// Sender-wide approximate pool balance; never sufficient for exact admission.
pub fn aggregate_balance<R: RegistryStorage>(
    reader: &mut R,
    caller: Address,
    native_balance: U256,
) -> Result<U256, RegistryError<R::Error>> {
    let mut total = native_balance;
    visit_eligible_tokens(reader, caller, |token, balance| {
        total = total.saturating_add(token.precision.aggregate(balance));
    })?;
    Ok(total)
}

#[cfg(test)]
mod tests;
