//!Handler related to Seismic chain
use crate::{
    api::exec::SeismicContextTr,
    chain::seismic_chain::TokenFeeReserve,
    gas_token_registry::{
        balance_storage_key, select_payment, GasPayment, GasToken, RegistryError, RegistryStorage,
        SelectedPayment,
    },
    transaction::abstraction::SeismicTxTr,
};
use revm::{
    context::{
        result::{ExecutionResult, InvalidTransaction},
        Block as _, Cfg as _, ContextTr, JournalTr, LocalContextTr,
    },
    context_interface::{
        context::ContextError,
        result::{FromStringError, HaltReason},
        transaction::Transaction,
    },
    handler::{
        handler::EvmTrError, post_execution, pre_execution::validate_account_nonce_and_code,
        EthFrame, EvmTr, FrameResult, FrameTr, Handler, MainnetHandler,
    },
    inspector::{Inspector, InspectorEvmTr, InspectorHandler},
    interpreter::{
        interpreter::EthInterpreter, interpreter_action::FrameInit, CallOutcome, Gas,
        InitialAndFloorGas, InstructionResult, InterpreterResult,
    },
    primitives::{Address, Bytes, FlaggedStorage, U256},
    Database,
};

/// Adapt the current journal to shared registry decoding without warming reads.
struct JournalRegistry<'a, J>(&'a mut J);

impl<J: JournalTr> RegistryStorage for JournalRegistry<'_, J> {
    type Error = <J::Database as Database>::Error;

    fn read_storage(&mut self, address: Address, key: U256) -> Result<FlaggedStorage, Self::Error> {
        self.0
            .system_storage(address, key)
            .map(|slot| FlaggedStorage::new(slot.data, slot.is_private))
    }
}

/// Apply one nonzero debit/credit with checked arithmetic and current-slot guards.
/// The system write journals an introduced touch, but never warms the token or slot.
fn token_balance_change<CTX, ERROR>(
    context: &mut CTX,
    token: GasToken,
    account: Address,
    amount: U256,
    debit: bool,
) -> Result<(), ERROR>
where
    CTX: ContextTr,
    ERROR: From<InvalidTransaction> + From<<CTX::Db as Database>::Error>,
{
    if amount.is_zero() {
        return Ok(());
    }
    let key = balance_storage_key(account, token.balance_slot);
    let slot = context.journal_mut().system_storage(token.token, key)?;
    let balance = FlaggedStorage::new(slot.data, slot.is_private);
    if !token.mode.accepts(balance) {
        return Err(InvalidTransaction::GasTokenBalanceModeMismatch {
            token: token.token,
            account,
        }
        .into());
    }
    let new = if debit {
        balance.value.checked_sub(amount).ok_or_else(|| {
            InvalidTransaction::LackOfFundForMaxFee {
                fee: Box::new(amount),
                balance: Box::new(balance.value),
            }
        })?
    } else {
        balance
            .value
            .checked_add(amount)
            .ok_or(InvalidTransaction::OverflowPaymentInTransaction)?
    };
    context.journal_mut().system_store(
        token.token,
        key,
        FlaggedStorage::new(new, token.mode.is_private()),
    )?;
    Ok(())
}

/// Surface body database errors before deterministic settlement checks can mask them.
fn pending_context_error<CTX, ERROR>(context: &mut CTX) -> Result<(), ERROR>
where
    CTX: ContextTr,
    ERROR: From<<CTX::Db as Database>::Error> + FromStringError,
{
    match core::mem::replace(context.error(), Ok(())) {
        Err(ContextError::Db(error)) => Err(error.into()),
        Err(ContextError::Custom(error)) => Err(ERROR::from_string(error)),
        Ok(()) => Ok(()),
    }
}

pub struct SeismicHandler<EVM, ERROR, FRAME> {
    pub mainnet: MainnetHandler<EVM, ERROR, FRAME>,
    pub _phantom: core::marker::PhantomData<(EVM, ERROR, FRAME)>,
}

impl<EVM, ERROR, FRAME> SeismicHandler<EVM, ERROR, FRAME> {
    pub fn new() -> Self {
        Self {
            mainnet: MainnetHandler::default(),
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<EVM, ERROR, FRAME> Default for SeismicHandler<EVM, ERROR, FRAME> {
    fn default() -> Self {
        Self::new()
    }
}

impl<EVM, ERROR, FRAME> Handler for SeismicHandler<EVM, ERROR, FRAME>
where
    EVM: EvmTr<Context: SeismicContextTr, Frame = FRAME>,
    ERROR: EvmTrError<EVM> + From<InvalidTransaction> + FromStringError + core::fmt::Debug,
    FRAME: FrameTr<FrameResult = FrameResult, FrameInit = FrameInit>,
{
    type Evm = EVM;
    type Error = ERROR;
    type HaltReason = HaltReason;

    /// Overrides the execution phase to short-circuit when a transaction's calldata
    /// decryption has failed. In this case, bytecode execution is skipped and a Revert
    /// is returned with all execution gas unspent. Final gas charges are determined by
    /// intrinsic gas, refund processing, and the applicable calldata gas floor.
    ///
    /// The validate and pre_execution phases still run normally, deducting gas fees
    /// and consuming the nonce for calls. Failed-decryption creations consume their
    /// nonce here because creation-frame initialization is skipped. The post_execution
    /// phase finalizes refunds, enforces the gas floor, reimburses unused gas, and
    /// credits the coinbase.
    #[inline]
    fn execution(
        &mut self,
        evm: &mut Self::Evm,
        init_and_floor_gas: &InitialAndFloorGas,
    ) -> Result<FrameResult, Self::Error> {
        if evm.ctx().tx().decryption_failed() {
            // A skipped CREATE never reaches make_create_frame's nonce bump.
            // Journal it so transaction-level errors can undo it, while the
            // successfully processed synthetic revert still consumes the nonce.
            let context = evm.ctx();
            if context.tx().kind().is_create() {
                let caller = context.tx().caller();
                let caller_account = context.journal_mut().load_account(caller)?.data;
                caller_account.info.nonce = caller_account
                    .info
                    .nonce
                    .checked_add(1)
                    .ok_or(InvalidTransaction::NonceOverflowInTransaction)?;
                context.journal_mut().nonce_bump_journal_entry(caller);
            }
            // Leave execution gas unspent; post-execution still enforces the gas floor.
            let execution_gas = evm.ctx().tx().gas_limit() - init_and_floor_gas.initial_gas;
            let mut frame_result = FrameResult::Call(CallOutcome::new(
                InterpreterResult {
                    result: InstructionResult::Revert,
                    output: Bytes::new(),
                    gas: Gas::new(execution_gas),
                },
                0..0,
            ));
            // Normalize frame gas to the transaction limit before fee settlement.
            self.last_frame_result(evm, &mut frame_result)?;
            return Ok(frame_result);
        }
        self.mainnet.execution(evm, init_and_floor_gas)
    }

    fn validate_against_state_and_deduct_caller(&self, evm: &mut Self::Evm) -> Result<(), ERROR> {
        let context = evm.ctx();
        context.chain_mut().clear_token_fee();
        let basefee = context.block().basefee() as u128;
        let blob_price = context.block().blob_gasprice().unwrap_or_default();
        let disabled = context.cfg().is_balance_check_disabled();
        let disable_code_check = context.cfg().is_eip3607_disabled();
        let disable_nonce_check = context.cfg().is_nonce_check_disabled();
        let caller = context.tx().caller();
        let value = context.tx().value();
        let selector = context.tx().gas_payment();
        if selector != GasPayment::Auto && context.tx().tx_type() != 74 {
            return Err(InvalidTransaction::InvalidGasPaymentSelector.into());
        }

        let (tx, journal) = context.tx_journal_mut();
        let account = journal.load_account_code(caller)?.data;
        validate_account_nonce_and_code(
            &mut account.info,
            tx.nonce(),
            disable_code_check,
            disable_nonce_check,
        )?;
        let maximum = tx.max_balance_spending()?;
        let effective = tx.effective_balance_spending(basefee, blob_price)?;
        let native_balance = account.info.balance;
        let is_call = tx.kind().is_call();
        if is_call {
            account.info.nonce = account
                .info
                .nonce
                .checked_add(1)
                .ok_or(InvalidTransaction::NonceOverflowInTransaction)?;
        }
        // All paths, including disabled checks, journal the nonce and only a newly
        // introduced touch. Native balance may subsequently be debited under this entry.
        journal.caller_accounting_journal_entry(caller, native_balance, is_call);
        if disabled {
            return Ok(());
        }
        let selected = select_payment(
            &mut JournalRegistry(context.journal_mut()),
            selector,
            caller,
            native_balance,
            value,
            maximum - value,
        )
        .map_err(|error| match error {
            RegistryError::Storage(error) => ERROR::from(error),
            RegistryError::Transaction(error) => ERROR::from(error),
        })?;
        let upfront_wei = effective - value;
        match selected {
            SelectedPayment::Native => {
                let account = context.journal_mut().load_account(caller)?.data;
                account.info.balance = native_balance
                    .checked_sub(upfront_wei)
                    .ok_or(InvalidTransaction::OverflowPaymentInTransaction)?;
            }
            SelectedPayment::Token(token) => {
                let upfront = token.precision.ceil(upfront_wei);
                token_balance_change::<_, ERROR>(context, token, caller, upfront, true)?;
                // Some with zero remaining must not become native fee accounting.
                context.chain_mut().set_token_fee(TokenFeeReserve {
                    token,
                    remaining: upfront,
                });
            }
        }
        Ok(())
    }

    fn post_execution(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut FrameResult,
        init_and_floor_gas: InitialAndFloorGas,
        eip7702_gas_refund: i64,
    ) -> Result<(), Self::Error> {
        pending_context_error::<_, ERROR>(evm.ctx())?;
        self.refund(evm, exec_result, eip7702_gas_refund);
        self.eip7623_check_gas_floor(evm, exec_result, init_and_floor_gas);
        self.reimburse_caller(evm, exec_result)?;
        self.reward_beneficiary(evm, exec_result)
    }

    fn reimburse_caller(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <<Self::Evm as EvmTr>::Frame as FrameTr>::FrameResult,
    ) -> Result<(), Self::Error> {
        let context = evm.ctx();
        pending_context_error::<_, ERROR>(context)?;
        if context.cfg().is_balance_check_disabled() {
            return Ok(());
        }
        if let Some(mut reserve) = context.chain().token_fee() {
            let price = U256::from(
                context
                    .tx()
                    .effective_gas_price(context.block().basefee() as u128),
            );
            let gas = exec_result.gas();
            let refunded = u64::try_from(gas.refunded())
                .map_err(|_| InvalidTransaction::OverflowPaymentInTransaction)?;
            let unused = gas
                .remaining()
                .checked_add(refunded)
                .ok_or(InvalidTransaction::OverflowPaymentInTransaction)?;
            let refund = reserve.token.precision.floor(price * U256::from(unused));
            reserve.remaining = reserve
                .remaining
                .checked_sub(refund)
                .ok_or(InvalidTransaction::OverflowPaymentInTransaction)?;
            token_balance_change::<_, ERROR>(
                context,
                reserve.token,
                context.tx().caller(),
                refund,
                false,
            )?;
            context.chain_mut().set_token_fee(reserve);
        } else {
            post_execution::reimburse_caller(context, exec_result.gas(), U256::ZERO)?;
        }
        Ok(())
    }

    fn reward_beneficiary(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <<Self::Evm as EvmTr>::Frame as FrameTr>::FrameResult,
    ) -> Result<(), Self::Error> {
        let context = evm.ctx();
        pending_context_error::<_, ERROR>(context)?;
        if context.cfg().is_balance_check_disabled() {
            return Ok(());
        }
        let beneficiary = context.block().beneficiary();
        if let Some(mut reserve) = context.chain().token_fee() {
            token_balance_change::<_, ERROR>(
                context,
                reserve.token,
                beneficiary,
                reserve.remaining,
                false,
            )?;
            reserve.remaining = U256::ZERO;
            context.chain_mut().set_token_fee(reserve);
        } else {
            // Native Seismic fees pay the full effective price, without a basefee burn.
            let price = U256::from(
                context
                    .tx()
                    .effective_gas_price(context.block().basefee() as u128),
            );
            context
                .journal_mut()
                .balance_incr(beneficiary, price * U256::from(exec_result.gas().used()))?;
        }
        Ok(())
    }

    /// Processes the final execution output.
    ///
    /// Retrieves the final state from the journal, converts internal results
    /// to the external output format. Internal state is cleared and EVM is
    /// prepared for the next transaction.
    ///
    /// Seismic Addendum:
    /// Given that we can't yet pass instruction_result which aren't in the
    /// InstructionResult enum, we leverage context_error to bubble up our
    /// instruction set specific errors.
    #[inline]
    fn execution_result(
        &mut self,
        evm: &mut Self::Evm,
        result: <<Self::Evm as EvmTr>::Frame as FrameTr>::FrameResult,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        pending_context_error::<_, ERROR>(evm.ctx())?;

        let exec_result = post_execution::output(evm.ctx(), result);

        evm.ctx().journal_mut().commit_tx();
        evm.ctx().local_mut().clear();
        evm.frame_stack().clear();
        evm.ctx().chain_mut().clear_token_fee();

        Ok(exec_result)
    }

    /// Handles cleanup when an error occurs during execution.
    ///
    /// Ensures the journal state is properly cleared before propagating the error.
    /// On the happy path the journal is committed in [`Handler::execution_result`].
    #[inline]
    fn catch_error(
        &self,
        evm: &mut Self::Evm,
        error: Self::Error,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        evm.ctx().local_mut().clear();
        evm.ctx().journal_mut().discard_tx();
        evm.frame_stack().clear();
        evm.ctx().chain_mut().clear_token_fee();
        *evm.ctx().error() = Ok(());
        Err(error)
    }
}

// Fix for the first error: Simplify the InspectorHandler implementation with proper bounds
impl<EVM, ERROR> InspectorHandler for SeismicHandler<EVM, ERROR, EthFrame<EthInterpreter>>
where
    EVM: InspectorEvmTr<Context: SeismicContextTr>,
    ERROR: EvmTrError<EVM> + From<InvalidTransaction> + FromStringError + core::fmt::Debug,
    EVM::Inspector: Inspector<EVM::Context, EthInterpreter>,
{
    type IT = EthInterpreter;

    /// Apply the failed-decryption guard before initializing any inspected frame.
    #[inline]
    fn inspect_execution(
        &mut self,
        evm: &mut Self::Evm,
        init_and_floor_gas: &InitialAndFloorGas,
    ) -> Result<FrameResult, Self::Error> {
        if evm.ctx().tx().decryption_failed() {
            // Reuse the synthetic revert and gas normalization from ordinary execution.
            return <Self as Handler>::execution(self, evm, init_and_floor_gas);
        }

        self.mainnet.inspect_execution(evm, init_and_floor_gas)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic
    )]

    use super::*;
    use crate::gas_token_registry::{
        token_metadata_slot, BalanceStorageMode, TokenPrecision, GAS_TOKEN_REGISTRY,
        TOKEN_COUNT_SLOT,
    };
    use crate::{
        api::default_ctx::SeismicContext, DefaultSeismicContext, SeismicBuilder, SeismicSpecId,
    };
    use revm::primitives::{address, keccak256};
    use revm::{
        context::{result::EVMError, CfgEnv, Context},
        context_interface::context::ContextError,
        database::InMemoryDB,
        database_interface::EmptyDB,
        handler::EthFrame,
        interpreter::{CallOutcome, Gas, InstructionResult, InterpreterResult},
        primitives::{Bytes, TxKind},
        state::AccountInfo,
        ExecuteEvm,
    };
    use rstest::rstest;

    // Six-decimal fixtures are registered explicitly; no production hardcoded token remains.
    const TOKEN: Address = address!("790701048922E265105fd6a4467a2901c2201C43");

    fn erc_address_storage(account: Address) -> U256 {
        balance_storage_key(account, U256::from(3))
    }

    fn test_token() -> GasToken {
        GasToken {
            token: TOKEN,
            balance_slot: U256::from(3),
            mode: BalanceStorageMode::Public,
            precision: TokenPrecision::new(6).unwrap(),
        }
    }

    fn token_operation<CTX: ContextTr, ERROR>(
        context: &mut CTX,
        sender: Address,
        recipient: Address,
        amount: U256,
    ) -> Result<(), ERROR>
    where
        ERROR: From<InvalidTransaction> + From<<CTX::Db as Database>::Error>,
    {
        token_balance_change::<_, ERROR>(context, test_token(), sender, amount, true)?;
        token_balance_change::<_, ERROR>(context, test_token(), recipient, amount, false)
    }

    /// Creates frame result.
    fn call_last_frame_return(
        ctx: SeismicContext<EmptyDB>,
        instruction_result: InstructionResult,
        gas: Gas,
    ) -> Gas {
        let mut evm = ctx.build_seismic_evm();

        let mut exec_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: instruction_result,
                output: Bytes::new(),
                gas,
            },
            0..0,
        ));

        let mut handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        handler
            .last_frame_result(&mut evm, &mut exec_result)
            .unwrap();
        handler.refund(&mut evm, &mut exec_result, 0);
        *exec_result.gas()
    }

    #[test]
    fn test_revert_gas() {
        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let gas = call_last_frame_return(ctx, InstructionResult::Revert, Gas::new(90));
        assert_eq!(gas.remaining(), 90);
        assert_eq!(gas.spent(), 10);
        assert_eq!(gas.refunded(), 0);
    }

    #[test]
    fn test_fatal_external_error_gas() {
        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let gas = call_last_frame_return(ctx, InstructionResult::FatalExternalError, Gas::new(90));
        assert_eq!(gas.remaining(), 0);
        assert_eq!(gas.spent(), 100);
        assert_eq!(gas.refunded(), 0);
    }

    /// Regression test: catch_error must call discard_tx() so that
    /// transaction_id advances and warm slot/account tracking is reset.
    /// Without discard_tx(), slots warmed in a failed tx would remain warm
    /// for the next tx, causing incorrect (cheaper) gas accounting.
    #[test]
    fn test_catch_error_discards_tx_and_advances_transaction_id() {
        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let mut evm = ctx.build_seismic_evm();

        let tx_id_before = evm.ctx().journal().inner.transaction_id;

        // Inject a context error that triggers the catch_error path.
        *evm.ctx().error() = Err(ContextError::Custom("some error".to_string()));

        let frame_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: InstructionResult::Stop,
                output: Bytes::new(),
                gas: Gas::new(90),
            },
            0..0,
        ));

        let mut handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        // execution_result returns Err for context errors, which the
        // execution loop passes to catch_error.
        let err = handler
            .execution_result(&mut evm, frame_result)
            .unwrap_err();
        let _ = handler.catch_error(&mut evm, err);

        // transaction_id must have advanced, proving discard_tx() was called.
        let tx_id_after = evm.ctx().journal().inner.transaction_id;
        assert_eq!(
            tx_id_after,
            tx_id_before + 1,
            "transaction_id should advance after catch_error (discard_tx must be called)"
        );
    }

    /// When `decryption_failed` is set on the transaction, the execution phase
    /// should short-circuit with a Revert and return all execution gas as
    /// remaining (only intrinsic gas is consumed).
    #[test]
    fn test_decryption_failed_skips_execution_and_returns_all_execution_gas() {
        let gas_limit: u64 = 100_000;
        let intrinsic_gas: u64 = 21_000;

        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = gas_limit;
            tx.decryption_failed = true;
        });

        let mut evm = ctx.build_seismic_evm();

        let mut handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        let init_and_floor_gas = InitialAndFloorGas::new(intrinsic_gas, 0);
        let result = handler.execution(&mut evm, &init_and_floor_gas).unwrap();

        let gas = result.gas();
        let execution_budget = gas_limit - intrinsic_gas;
        assert_eq!(
            gas.remaining(),
            execution_budget,
            "all execution gas should be remaining (returned to sender)"
        );
        assert_eq!(gas.limit(), gas_limit, "gas must use the transaction limit");
        assert_eq!(
            gas.spent(),
            intrinsic_gas,
            "only intrinsic gas should be spent"
        );
        assert_eq!(gas.refunded(), 0, "revert must not retain refund credits");

        // Verify it's a Revert
        match &result {
            FrameResult::Call(outcome) => {
                assert_eq!(
                    outcome.result.result,
                    InstructionResult::Revert,
                    "should be a Revert"
                );
                assert!(
                    outcome.result.output.is_empty(),
                    "revert output should be empty"
                );
            }
            _ => panic!("expected FrameResult::Call"),
        }
    }

    /// Failed decryption must charge intrinsic gas subject to the Prague floor,
    /// return unused gas, and pay the beneficiary the exact native fee. Exercise
    /// the full transaction path, since checking only execution gas misses the
    /// normalization needed before post-execution accounting.
    #[rstest]
    #[case::empty_minimum(0, 21_000, 21_000)]
    #[case::empty_intermediate(0, 30_000, 21_000)]
    #[case::empty_large(0, 100_000, 21_000)]
    #[case::nonempty_minimum(16, 21_640, 21_640)]
    #[case::nonempty_intermediate(16, 30_000, 21_640)]
    #[case::nonempty_large(16, 100_000, 21_640)]
    fn test_decryption_failed_native_gas_accounting(
        #[case] data_len: usize,
        #[case] gas_limit: u64,
        #[case] expected_gas_used: u64,
        #[values((0, 0), (1, 0), (3, 1))] gas_price_and_basefee: (u128, u64),
        #[values(false, true)] decryption_failed: bool,
    ) {
        // A nonzero basefee distinguishes Seismic's full-price reward from tip-only payment.
        let (gas_price, basefee) = gas_price_and_basefee;
        let caller = address!("0x0000000000000000000000000000000000001234");
        let beneficiary = address!("0x0000000000000000000000000000000000005678");
        let initial_caller_balance = U256::from(1_000_000u64);
        let initial_beneficiary_balance = U256::from(50_000u64);
        let mut db = InMemoryDB::default();
        db.insert_account_info(
            caller,
            AccountInfo {
                balance: initial_caller_balance,
                ..Default::default()
            },
        );
        db.insert_account_info(
            beneficiary,
            AccountInfo {
                balance: initial_beneficiary_balance,
                ..Default::default()
            },
        );

        // Mercury enables Prague: 16 nonzero bytes cost 21,256 intrinsic gas,
        // but the 21,640 calldata floor determines the final charge here.
        let ctx = Context::seismic_with_random_rng_key()
            .with_cfg(CfgEnv::new_with_spec(SeismicSpecId::MERCURY))
            .modify_block_chained(|block| {
                block.beneficiary = beneficiary;
                block.basefee = basefee;
            })
            .modify_tx_chained(|tx| {
                tx.base.caller = caller;
                tx.base.nonce = 0;
                tx.base.gas_limit = gas_limit;
                tx.base.gas_price = gas_price;
                tx.base.gas_priority_fee = None;
                tx.base.value = U256::ZERO;
                tx.base.kind = TxKind::Call(Address::ZERO);
                tx.base.data = Bytes::from(vec![0xff; data_len]);
                tx.decryption_failed = decryption_failed;
            })
            .with_db(db);
        let mut evm = ctx.build_seismic_evm();
        let result = evm.replay().unwrap();

        if decryption_failed {
            assert!(matches!(
                &result.result,
                ExecutionResult::Revert { output, .. } if output.is_empty()
            ));
        } else {
            assert!(result.result.is_success());
        }

        let caller_account = &result.state[&caller];
        assert_eq!(caller_account.info.nonce, 1, "nonce must be consumed");
        let caller_charged = initial_caller_balance - caller_account.info.balance;
        let upfront_fee = U256::from(gas_price * u128::from(gas_limit));
        let caller_reimbursed =
            caller_account.info.balance - (initial_caller_balance - upfront_fee);
        let beneficiary_paid = result
            .state
            .get(&beneficiary)
            .map(|account| account.info.balance)
            .unwrap_or(initial_beneficiary_balance)
            - initial_beneficiary_balance;
        let expected_fee = U256::from(gas_price * u128::from(expected_gas_used));
        let expected_reimbursement =
            U256::from(gas_price * u128::from(gas_limit - expected_gas_used));

        assert_eq!(
            (
                result.result.gas_used(),
                caller_charged,
                caller_reimbursed,
                beneficiary_paid,
            ),
            (
                expected_gas_used,
                expected_fee,
                expected_reimbursement,
                expected_fee,
            ),
            "(gas used, caller charged, caller reimbursed, beneficiary paid) must match protocol accounting"
        );
    }

    /// When `decryption_failed` is NOT set, execution should proceed normally
    /// (delegating to the mainnet handler). This verifies the flag check doesn't
    /// accidentally short-circuit normal transactions.
    #[test]
    fn test_decryption_not_failed_proceeds_normally() {
        let ctx = Context::seismic_with_random_rng_key().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
            // decryption_failed defaults to false
        });

        let mut evm = ctx.build_seismic_evm();

        let mut handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        let init_and_floor_gas = InitialAndFloorGas::new(21, 0);
        // Normal execution — this will attempt to run a transaction against
        // the empty DB, which may fail, but the point is it does NOT take
        // the decryption_failed short-circuit path.
        let result = handler.execution(&mut evm, &init_and_floor_gas);

        // If execution reached the mainnet handler (not short-circuited),
        // the result might be Ok or Err depending on the empty state, but
        // it should NOT be the specific Revert-with-full-gas pattern from
        // the decryption_failed path.
        if let Ok(frame_result) = result {
            let gas = frame_result.gas();
            // Normal execution would consume some gas, not return full budget
            let execution_budget = 100 - 21;
            // If it returned full budget as remaining, that means it hit the
            // decryption_failed path (which it shouldn't)
            if gas.remaining() == execution_budget && gas.spent() == 0 {
                panic!("normal tx should not take the decryption_failed short-circuit path");
            }
        }
        // Any other result (error, different gas values) means normal execution
        // was attempted, which is correct.
    }

    // ==================== ERC20 Gas Tests ====================

    /// Build a SeismicContext on an InMemoryDB with the TOKEN contract account
    /// pre-created, the caller seeded with `eth_balance` ETH and `usdc_balance`
    /// USDC, and a tx with the given gas_limit, gas_price, and value.
    fn build_erc20_ctx(
        caller: Address,
        eth_balance: U256,
        usdc_balance: U256,
        gas_limit: u64,
        gas_price: u128,
        value: U256,
    ) -> SeismicContext<InMemoryDB> {
        let mut db = InMemoryDB::default();

        // Seed caller's ETH balance.
        db.insert_account_info(
            caller,
            AccountInfo {
                balance: eth_balance,
                ..Default::default()
            },
        );

        // Register the six-decimal fixture as Public; do not convert public balances to private.
        db.insert_account_info(
            TOKEN,
            AccountInfo {
                nonce: 1,
                ..Default::default()
            },
        );
        db.insert_account_info(
            GAS_TOKEN_REGISTRY,
            AccountInfo {
                nonce: 1,
                ..Default::default()
            },
        );
        db.insert_account_storage(GAS_TOKEN_REGISTRY, TOKEN_COUNT_SLOT, U256::from(1).into())
            .unwrap();
        let metadata = U256::from_be_slice(TOKEN.as_slice())
            | (U256::from(1) << 160usize)
            | (U256::from(1) << 168usize)
            | (U256::from(6) << 176usize);
        db.insert_account_storage(GAS_TOKEN_REGISTRY, token_metadata_slot(0), metadata.into())
            .unwrap();
        db.insert_account_storage(
            GAS_TOKEN_REGISTRY,
            token_metadata_slot(0) + U256::from(1),
            U256::from(3).into(),
        )
        .unwrap();

        // Seed caller's USDC balance in the TOKEN storage.
        if usdc_balance > U256::ZERO {
            let slot = erc_address_storage(caller);
            db.insert_account_storage(TOKEN, slot, usdc_balance.into())
                .unwrap();
        }

        Context::seismic_with_random_rng_key()
            .modify_tx_chained(|tx| {
                tx.base.caller = caller;
                tx.base.gas_limit = gas_limit;
                tx.base.gas_price = gas_price;
                tx.base.gas_priority_fee = None;
                tx.base.value = value;
                tx.base.kind = TxKind::Call(Address::ZERO);
            })
            .with_db(db)
    }

    /// Read the USDC balance of `addr` from the journal.
    fn read_usdc_balance<CTX: ContextTr>(ctx: &mut CTX, addr: Address) -> U256
    where
        <CTX::Db as Database>::Error: core::fmt::Debug,
    {
        let slot = erc_address_storage(addr);
        ctx.journal_mut().system_storage(TOKEN, slot).unwrap().data
    }

    /// Failed decryption must settle token-funded gas through the full transaction path,
    /// preserving the ceil-upfront/floor-reimbursement policy and paying the full price.
    #[rstest]
    #[case::empty_minimum(0, 21_000, 21_000, 21)]
    #[case::empty_large(0, 100_000, 21_000, 21)]
    #[case::empty_rounded(0, 100_001, 21_000, 22)]
    #[case::nonempty_minimum(16, 21_640, 21_640, 22)]
    #[case::nonempty_large(16, 100_000, 21_640, 22)]
    #[case::nonempty_rounded(16, 100_001, 21_640, 23)]
    fn test_decryption_failed_erc20_gas_accounting(
        #[case] data_len: usize,
        #[case] gas_limit: u64,
        #[case] expected_gas_used: u64,
        #[case] expected_token_charge: u64,
    ) {
        let caller = address!("0x0000000000000000000000000000000000001234");
        let beneficiary = address!("0x0000000000000000000000000000000000005678");
        let initial_caller_tokens = U256::from(1_000u64);
        let initial_beneficiary_tokens = U256::from(50u64);
        let mut ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            initial_caller_tokens,
            gas_limit,
            1_000_000_000,
            U256::ZERO,
        )
        .with_cfg(CfgEnv::new_with_spec(SeismicSpecId::MERCURY))
        .modify_block_chained(|block| {
            block.beneficiary = beneficiary;
            block.basefee = 500_000_000;
        })
        .modify_tx_chained(|tx| {
            tx.base.nonce = 0;
            tx.base.data = Bytes::from(vec![0xff; data_len]);
            tx.decryption_failed = true;
        });
        ctx.db_mut()
            .insert_account_storage(
                TOKEN,
                erc_address_storage(beneficiary),
                initial_beneficiary_tokens.into(),
            )
            .unwrap();
        let mut evm = ctx.build_seismic_evm();
        let result = evm.replay().unwrap();

        assert!(matches!(
            &result.result,
            ExecutionResult::Revert { output, .. } if output.is_empty()
        ));
        assert_eq!(result.result.gas_used(), expected_gas_used);
        assert_eq!(result.state[&caller].info.nonce, 1);
        assert_eq!(result.state[&caller].info.balance, U256::ZERO);
        assert_eq!(
            result
                .state
                .get(&beneficiary)
                .map(|account| account.info.balance)
                .unwrap_or_default(),
            U256::ZERO,
            "token-funded gas must not pay a native ETH reward"
        );

        // At 1 gwei, upfront deduction is ceil(gas_limit / 1000) token units
        // and reimbursement is floor((gas_limit - gas_used) / 1000).
        // E.g. nonempty_rounded deducts 101 and returns 78, for a net charge of 23.
        let token_storage = &result.state[&TOKEN].storage;
        let caller_tokens = token_storage[&erc_address_storage(caller)]
            .present_value
            .value;
        let beneficiary_tokens = token_storage[&erc_address_storage(beneficiary)]
            .present_value
            .value;
        let expected_charge = U256::from(expected_token_charge);
        assert_eq!(initial_caller_tokens - caller_tokens, expected_charge);
        assert_eq!(
            beneficiary_tokens - initial_beneficiary_tokens,
            expected_charge,
            "beneficiary must retain the full token fee, including rounding, with no burn"
        );
        assert_eq!(
            caller_tokens + beneficiary_tokens,
            initial_caller_tokens + initial_beneficiary_tokens,
            "token fees must conserve the initial supply"
        );
        assert!(
            !evm.ctx().chain().used_erc20_gas(),
            "the payment flag must be reset after settlement"
        );
    }

    #[test]
    fn test_erc20_gas_fallback_deducts_to_reserve() {
        // Caller has 0 native but enough tokens. The beneficiary gets no upfront credit.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000; // 1 gwei
        let usdc_balance = U256::from(1_000_000_000u64); // 1000 USDC

        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            usdc_balance,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(
            evm.ctx().chain().used_erc20_gas(),
            "should use ERC20 gas when ETH balance is insufficient"
        );

        // gas cost in wei = 100_000 * 1e9 = 1e14
        // gas cost in USDC = 1e14 / 1e12 = 100 USDC
        let expected = U256::from(100u64);
        let caller_after = read_usdc_balance(evm.ctx(), caller);
        let beneficiary_after = read_usdc_balance(evm.ctx(), beneficiary);
        assert_eq!(caller_after, usdc_balance - expected);
        assert_eq!(
            beneficiary_after,
            U256::ZERO,
            "upfront reserve must not be spendable by the beneficiary"
        );
    }

    #[test]
    fn test_eth_path_when_balance_sufficient() {
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        let eth_balance = U256::from(1_000_000_000_000_000u128); // 1e15, plenty

        let ctx = build_erc20_ctx(
            caller,
            eth_balance,
            U256::ZERO,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(
            !evm.ctx().chain().used_erc20_gas(),
            "sufficient ETH should use native path"
        );
    }

    #[test]
    fn test_erc20_gas_insufficient_usdc_balance_fails() {
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        // Need at least 100 USDC; give 50.
        let usdc_balance = U256::from(50u64);

        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            usdc_balance,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let result = handler.validate_against_state_and_deduct_caller(&mut evm);
        assert!(result.is_err());
    }

    #[test]
    fn test_erc20_gas_reimburse_from_reserve() {
        // Refunds credit the caller from the internal reserve, not the beneficiary.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        let usdc_balance = U256::from(1_000_000_000u64);

        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            usdc_balance,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        let beneficiary_before_reimburse = read_usdc_balance(evm.ctx(), beneficiary);
        let caller_before_reimburse = read_usdc_balance(evm.ctx(), caller);

        // Build a gas object with 50k remaining out of 79k execution budget.
        let mut gas = Gas::new(gas_limit - 21_000);
        let _ = gas.record_cost(29_000);
        let mut exec_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: InstructionResult::Stop,
                output: Bytes::new(),
                gas,
            },
            0..0,
        ));

        handler
            .reimburse_caller(&mut evm, &mut exec_result)
            .unwrap();

        // 50_000 * 1e9 / 1e12 = 50 USDC refunded.
        let expected_refund = U256::from(50u64);
        assert_eq!(
            read_usdc_balance(evm.ctx(), caller),
            caller_before_reimburse + expected_refund
        );
        assert_eq!(
            read_usdc_balance(evm.ctx(), beneficiary),
            beneficiary_before_reimburse
        );
    }

    #[test]
    fn test_erc20_gas_beneficiary_gets_net_reward_no_burn() {
        // End-to-end invariant test: deduct → reimburse → reward_beneficiary.
        // The key invariant: no tokens are burned. caller_final + beneficiary_final
        // must equal the initial USDC supply.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 2_000_000_000; // 2 gwei
        let usdc_balance = U256::from(1_000_000_000u64);

        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            usdc_balance,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let ctx = ctx.modify_block_chained(|b| {
            b.basefee = 1_000_000_000; // 1 gwei — would be burned on mainnet
        });
        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();

        // 1. Deduct 200 raw units into an internal reserve, not the beneficiary.
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();
        assert_eq!(read_usdc_balance(evm.ctx(), beneficiary), U256::ZERO);

        // 2. Reimburse: 50k gas remaining. 50_000 * 2e9 / 1e12 = 100 USDC back.
        let mut gas = Gas::new(gas_limit - 21_000);
        let _ = gas.record_cost(29_000);
        let mut exec_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: InstructionResult::Stop,
                output: Bytes::new(),
                gas,
            },
            0..0,
        ));
        handler
            .reimburse_caller(&mut evm, &mut exec_result)
            .unwrap();

        // 3. Reward the beneficiary with the 100-unit reserve remainder.
        let beneficiary_before_reward = read_usdc_balance(evm.ctx(), beneficiary);
        handler
            .reward_beneficiary(&mut evm, &mut exec_result)
            .unwrap();
        let beneficiary_after_reward = read_usdc_balance(evm.ctx(), beneficiary);
        assert_eq!(
            beneficiary_before_reward + U256::from(100),
            beneficiary_after_reward,
            "reward_beneficiary must credit the reserve remainder"
        );

        // Invariant: no tokens burned.
        let caller_final = read_usdc_balance(evm.ctx(), caller);
        let beneficiary_final = read_usdc_balance(evm.ctx(), beneficiary);
        assert_eq!(
            caller_final + beneficiary_final,
            usdc_balance,
            "no tokens should be burned: caller + beneficiary = initial supply"
        );
    }

    #[test]
    fn test_erc20_gas_with_value_checks_eth_for_value() {
        // Caller has 0 ETH but value > 0 — must fail (value transferred in ETH).
        let caller = address!("0x0000000000000000000000000000000000001234");
        let value = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
        let usdc_balance = U256::from(1_000_000_000u64);

        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            usdc_balance,
            100_000,
            1_000_000_000,
            value,
        );
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let result = handler.validate_against_state_and_deduct_caller(&mut evm);
        assert!(
            result.is_err(),
            "tx with value > 0 and 0 ETH should fail even with USDC"
        );
    }

    #[test]
    fn test_erc20_gas_with_value_succeeds_when_eth_covers_value() {
        // Caller has enough ETH for value (but not gas+value) and enough USDC
        // for gas only. Should succeed via ERC20 path.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let value = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
        let eth_balance = value; // exactly covers value
        let usdc_balance = U256::from(1_000_000_000u64);

        let ctx = build_erc20_ctx(
            caller,
            eth_balance,
            usdc_balance,
            100_000,
            1_000_000_000,
            value,
        );
        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(evm.ctx().chain().used_erc20_gas());
        // Only gas is deducted in USDC (100), not gas+value.
        let expected_gas_usdc = U256::from(100u64);
        assert_eq!(
            read_usdc_balance(evm.ctx(), caller),
            usdc_balance - expected_gas_usdc
        );
        assert_eq!(read_usdc_balance(evm.ctx(), beneficiary), U256::ZERO);
        assert_eq!(
            evm.ctx().chain().token_fee().unwrap().remaining,
            expected_gas_usdc
        );
    }

    #[test]
    fn test_erc20_gas_usdc_check_excludes_value() {
        // Regression: USDC check must only cover gas, not gas+value.
        // 100 USDC is exactly enough for gas. If value were included, it would
        // require 1_000_100 USDC and fail.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let value = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
        let eth_balance = value;
        let usdc_balance = U256::from(100u64);

        let ctx = build_erc20_ctx(
            caller,
            eth_balance,
            usdc_balance,
            100_000,
            1_000_000_000,
            value,
        );
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let result = handler.validate_against_state_and_deduct_caller(&mut evm);
        assert!(
            result.is_ok(),
            "USDC check should cover gas only, not value"
        );
    }

    #[test]
    fn test_erc20_gas_sub_divisor_charges_at_least_one_unit() {
        // Regression test for floor-division bug: when gas_balance_spending is
        // less than WEI_TO_USDC_DIVISOR (10^12 wei), floor division would yield
        // 0 USDC deducted, allowing free transactions. The fix uses ceiling
        // division so at least 1 unit of USDC is charged.
        //
        // Repro from writeup: baseFee ≈ 7 wei, gas_limit=200_000,
        // effective_gas_price ≈ 10^6 wei → gas_balance_spending = 2*10^11 wei,
        // which is < 10^12. Under floor: 0 USDC charged. Under ceil: 1 USDC.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 200_000;
        let gas_price: u128 = 1_000_000; // 10^6 wei — sub-divisor
        let usdc_balance = U256::from(100u64);

        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            usdc_balance,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        // gas_balance_spending = 200_000 * 10^6 = 2*10^11 wei (< 10^12).
        // Ceiling division: ceil(2*10^11 / 10^12) = 1 USDC unit.
        let caller_after = read_usdc_balance(evm.ctx(), caller);
        let beneficiary_after = read_usdc_balance(evm.ctx(), beneficiary);
        assert_eq!(
            caller_after,
            usdc_balance - U256::from(1u64),
            "caller must be charged at least 1 USDC unit for sub-divisor gas costs"
        );
        assert_eq!(
            beneficiary_after,
            U256::ZERO,
            "beneficiary must not receive the fee before execution"
        );
    }

    #[test]
    fn test_erc20_gas_ceil_deduct_matches_pre_flight_check() {
        // The deduction's ceiling division must produce the same value as the
        // pre-flight check's ceiling. If the caller has exactly max_gas_spending_usdc
        // in USDC, the deduction must succeed (not fail with insufficient balance).
        //
        // Scenario: gas_limit=100_001, gas_price=10^9 → gas_balance_spending =
        // 100_001_000_000_000 wei. ceil(./10^12) = 101 USDC.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_001;
        let gas_price: u128 = 1_000_000_000;
        let usdc_balance = U256::from(101u64);

        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            usdc_balance,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        // Exactly 101 raw units deducted: caller zero, reserve 101, beneficiary zero.
        assert_eq!(read_usdc_balance(evm.ctx(), caller), U256::ZERO);
        assert_eq!(read_usdc_balance(evm.ctx(), beneficiary), U256::ZERO);
        assert_eq!(
            evm.ctx().chain().token_fee().unwrap().remaining,
            U256::from(101)
        );
    }

    #[test]
    fn test_wei_to_usdc_ceiling_division() {
        // gas_limit=100_001 gives 100_001_000_000_000 wei; ceil(./1e12) = 101.
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_001;
        let gas_price: u128 = 1_000_000_000;

        // 100 USDC should NOT be enough.
        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            U256::from(100u64),
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();
        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        assert!(handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .is_err());

        // 101 USDC should be sufficient.
        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            U256::from(101u64),
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();
        assert!(handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .is_ok());
    }

    #[test]
    fn test_erc20_gas_exact_eth_boundary_uses_native() {
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        let exact_eth = U256::from(100_000_000_000_000u128); // = max_balance_spending

        let ctx = build_erc20_ctx(
            caller,
            exact_eth,
            U256::ZERO,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(!evm.ctx().chain().used_erc20_gas());
    }

    #[test]
    fn test_erc20_gas_one_wei_short_falls_to_erc20() {
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 1_000_000_000;
        let one_short = U256::from(100_000_000_000_000u128) - U256::from(1);
        let usdc_balance = U256::from(1_000_000_000u64);

        let ctx = build_erc20_ctx(
            caller,
            one_short,
            usdc_balance,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        assert!(evm.ctx().chain().used_erc20_gas());
    }

    #[test]
    fn test_native_eth_no_basefee_burn() {
        // Native ETH path with non-zero basefee. The beneficiary must receive
        // the full effective_gas_price * gas_used (no EIP-1559 burn).
        let caller = address!("0x0000000000000000000000000000000000001234");
        let gas_limit: u64 = 100_000;
        let gas_price: u128 = 2_000_000_000; // 2 gwei
        let eth_balance = U256::from(1_000_000_000_000_000_000u128); // 1 ETH

        let ctx = build_erc20_ctx(
            caller,
            eth_balance,
            U256::ZERO,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let ctx = ctx.modify_block_chained(|b| {
            b.basefee = 1_000_000_000; // 1 gwei
        });
        let mut evm = ctx.build_seismic_evm();
        let beneficiary = evm.ctx().block().beneficiary();

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        handler
            .validate_against_state_and_deduct_caller(&mut evm)
            .unwrap();

        // Simulate all 79k execution gas used.
        let mut gas = Gas::new(gas_limit - 21_000);
        let _ = gas.record_cost(gas_limit - 21_000);
        let mut exec_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: InstructionResult::Stop,
                output: Bytes::new(),
                gas,
            },
            0..0,
        ));

        let ben_balance_before = evm
            .ctx()
            .journal_mut()
            .load_account(beneficiary)
            .unwrap()
            .data
            .info
            .balance;

        handler
            .reward_beneficiary(&mut evm, &mut exec_result)
            .unwrap();

        let ben_balance_after = evm
            .ctx()
            .journal_mut()
            .load_account(beneficiary)
            .unwrap()
            .data
            .info
            .balance;

        // No basefee burn: reward = effective_gas_price * gas_used.
        // = 2 gwei * 79_000 = 1.58e14 wei.
        let expected_reward = U256::from(2_000_000_000u128 * 79_000u128);
        assert_eq!(
            ben_balance_after - ben_balance_before,
            expected_reward,
            "beneficiary must receive full effective_gas_price (no basefee burn)"
        );

        // Mainnet (with burn) would give half as much.
        let mainnet_reward = U256::from(1_000_000_000u128 * 79_000u128);
        assert!(expected_reward > mainnet_reward);
    }

    #[test]
    fn test_erc20_gas_flag_reset_on_execution_result() {
        let caller = address!("0x0000000000000000000000000000000000001234");
        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            U256::from(1_000_000_000u64),
            100_000,
            1_000_000_000,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();
        evm.ctx().chain_mut().set_token_fee(TokenFeeReserve {
            token: test_token(),
            remaining: U256::ZERO,
        });
        assert!(evm.ctx().chain().used_erc20_gas());

        let frame_result = FrameResult::Call(CallOutcome::new(
            InterpreterResult {
                result: InstructionResult::Stop,
                output: Bytes::new(),
                gas: Gas::new(0),
            },
            0..0,
        ));

        let mut handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let _ = handler.execution_result(&mut evm, frame_result);

        assert!(!evm.ctx().chain().used_erc20_gas());
    }

    #[test]
    fn test_erc20_gas_flag_reset_on_catch_error() {
        let caller = address!("0x0000000000000000000000000000000000001234");
        let ctx = build_erc20_ctx(
            caller,
            U256::ZERO,
            U256::from(1_000_000_000u64),
            100_000,
            1_000_000_000,
            U256::ZERO,
        );
        let mut evm = ctx.build_seismic_evm();
        evm.ctx().chain_mut().set_token_fee(TokenFeeReserve {
            token: test_token(),
            remaining: U256::ZERO,
        });

        let handler =
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<EthInterpreter>>::new();
        let err: EVMError<_, InvalidTransaction> = EVMError::Custom("test".to_string());
        let _ = handler.catch_error(&mut evm, err);

        assert!(!evm.ctx().chain().used_erc20_gas());
    }

    #[test]
    fn test_erc_address_storage_deterministic() {
        let addr = address!("0x0000000000000000000000000000000000001234");
        assert_eq!(erc_address_storage(addr), erc_address_storage(addr));
    }

    #[test]
    fn test_erc_address_storage_different_addresses() {
        let a = address!("0x0000000000000000000000000000000000001234");
        let b = address!("0x0000000000000000000000000000000000005678");
        assert_ne!(erc_address_storage(a), erc_address_storage(b));
    }

    #[test]
    fn test_erc_address_storage_matches_solidity_mapping_layout() {
        // Pin the expected slot: standard Solidity `mapping(address => uint256)`
        // at position 3 computes slot as keccak256(addr_32 || slot_32).
        // Use web3-style hand computation to guard against accidental layout
        // changes (e.g., back to Solady) in the future.
        let addr = address!("0x1234567890abcdef1234567890abcdef12345678");

        let mut expected_input = [0u8; 64];
        expected_input[12..32].copy_from_slice(addr.as_slice());
        expected_input[63] = 3; // BALANCES_SLOT
        let expected: U256 = keccak256(expected_input).into();

        assert_eq!(erc_address_storage(addr), expected);
    }

    #[test]
    fn test_token_operation_transfer() {
        let sender = address!("0x0000000000000000000000000000000000001111");
        let recipient = address!("0x0000000000000000000000000000000000002222");
        let initial = U256::from(1000u64);
        let amount = U256::from(300u64);

        let mut db = InMemoryDB::default();
        db.insert_account_info(TOKEN, AccountInfo::default());
        db.insert_account_info(sender, AccountInfo::default());
        db.insert_account_info(recipient, AccountInfo::default());
        db.insert_account_storage(TOKEN, erc_address_storage(sender), initial.into())
            .unwrap();

        let ctx: SeismicContext<InMemoryDB> = Context::seismic_with_random_rng_key()
            .modify_tx_chained(|tx| {
                tx.base.caller = sender;
                tx.base.gas_limit = 100_000;
                tx.base.kind = TxKind::Call(Address::ZERO);
            })
            .with_db(db);
        let mut ctx = ctx;
        ctx.journal_mut().load_account(TOKEN).unwrap();

        token_operation::<_, EVMError<_, InvalidTransaction>>(&mut ctx, sender, recipient, amount)
            .unwrap();

        assert_eq!(read_usdc_balance(&mut ctx, sender), initial - amount);
        assert_eq!(read_usdc_balance(&mut ctx, recipient), amount);
    }

    #[test]
    fn test_token_operation_insufficient_balance() {
        let sender = address!("0x0000000000000000000000000000000000001111");
        let recipient = address!("0x0000000000000000000000000000000000002222");

        let mut db = InMemoryDB::default();
        db.insert_account_info(TOKEN, AccountInfo::default());
        db.insert_account_info(sender, AccountInfo::default());
        db.insert_account_info(recipient, AccountInfo::default());
        db.insert_account_storage(
            TOKEN,
            erc_address_storage(sender),
            U256::from(100u64).into(),
        )
        .unwrap();

        let ctx: SeismicContext<InMemoryDB> = Context::seismic_with_random_rng_key()
            .modify_tx_chained(|tx| {
                tx.base.caller = sender;
                tx.base.gas_limit = 100_000;
                tx.base.kind = TxKind::Call(Address::ZERO);
            })
            .with_db(db);
        let mut ctx = ctx;
        ctx.journal_mut().load_account(TOKEN).unwrap();

        let result = token_operation::<_, EVMError<_, InvalidTransaction>>(
            &mut ctx,
            sender,
            recipient,
            U256::from(200u64),
        );
        assert!(result.is_err());
    }
}
