//!Handler related to Seismic chain
use crate::src20_gas::{
    gas_balance_of, gas_caller_key, gas_set_balance, gas_token_operation, GAS_SRC20_ADDRESS,
    TREASURY,
};
use crate::{api::exec::SeismicContextTr, SeismicHaltReason};
use revm::{
    context::{
        result::{ExecutionResult, InvalidTransaction, ResultAndState},
        ContextTr, JournalTr, Transaction,
    },
    context_interface::{context::ContextError, result::FromStringError, Block, Cfg},
    handler::{
        handler::EvmTrError, post_execution, pre_execution::validate_account_nonce_and_code, EvmTr,
        Frame, FrameResult, Handler, MainnetHandler,
    },
    inspector::{Inspector, InspectorEvmTr, InspectorFrame, InspectorHandler},
    interpreter::{interpreter::EthInterpreter, FrameInput, InstructionResult},
    primitives::U256,
};

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
    EVM: EvmTr<Context: SeismicContextTr>,
    ERROR: EvmTrError<EVM> + From<InvalidTransaction> + FromStringError,
    FRAME: Frame<Evm = EVM, Error = ERROR, FrameResult = FrameResult, FrameInit = FrameInput>,
{
    type Evm = EVM;
    type Error = ERROR;
    type Frame = FRAME;
    type HaltReason = SeismicHaltReason;

    /// Validates the transaction against the state and deducts gas from the caller
    fn validate_against_state_and_deduct_caller(&self, evm: &mut Self::Evm) -> Result<(), ERROR> {
        let context = evm.ctx();
        let basefee = context.block().basefee() as u128;
        let blob_price = context.block().blob_gasprice().unwrap_or_default();
        let is_balance_check_disabled = context.cfg().is_balance_check_disabled();
        let is_eip3607_disabled = context.cfg().is_eip3607_disabled();
        let is_nonce_check_disabled = context.cfg().is_nonce_check_disabled();
        let caller = context.tx().caller();
        let value = context.tx().value();

        let (tx, journal) = context.tx_journal();

        // Load caller's account.
        let caller_account = journal.load_account_code(tx.caller())?.data;

        validate_account_nonce_and_code(
            &mut caller_account.info,
            tx.nonce(),
            tx.kind().is_call(),
            is_eip3607_disabled,
            is_nonce_check_disabled,
        )?;

        // Touch account so we know it is changed.
        caller_account.mark_touch();

        let max_balance_spending = tx.max_balance_spending()?;
        let effective_balance_spending = tx
            .effective_balance_spending(basefee, blob_price)
            .expect("effective balance is always smaller than max balance so it can't overflow");

        // Load the SRC20 Gas contract into the journal and mark it as touched,
        // as the value should always update
        // Then validate the caller's balance
        let caller_slot = gas_caller_key(caller);
        context
            .journal()
            .warm_account_and_storage(GAS_SRC20_ADDRESS, [caller_slot])
            .unwrap();
        context.journal().touch_account(GAS_SRC20_ADDRESS);
        
        let account_balance = gas_balance_of::<EVM::Context, ERROR>(context, caller)?;

        if account_balance < max_balance_spending && !is_balance_check_disabled {
            return Err(InvalidTransaction::LackOfFundForMaxFee {
                fee: Box::new(max_balance_spending),
                balance: Box::new(account_balance),
            }
            .into());
        };

        // Check if account has enough balance for `gas_limit * max_fee`` and value transfer.
        // Transfer will be done inside `*_inner` functions.
        if is_balance_check_disabled {
            // TODO: adjust with GAS_SRC20_CONVERATION_RATIO?
            let temp_caller_balance =
                gas_balance_of::<EVM::Context, ERROR>(context, caller)?.max(max_balance_spending);
            gas_set_balance::<EVM::Context, ERROR>(context, caller, temp_caller_balance)?;
        } else if max_balance_spending > account_balance {
            return Err(InvalidTransaction::LackOfFundForMaxFee {
                fee: Box::new(max_balance_spending),
                balance: Box::new(account_balance),
            }
            .into());
        } else {
            // subtracting max balance spending with value that is going to be deducted later in the call.
            let gas_balance_spending = effective_balance_spending - value;

            gas_token_operation::<EVM::Context, ERROR>(
                context,
                caller,
                TREASURY,
                gas_balance_spending,
            )?;
        }

        Ok(())
    }

    /// Reimburses the caller for unused gas
    fn reimburse_caller(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <Self::Frame as Frame>::FrameResult,
    ) -> Result<(), Self::Error> {
        println!("entered reimburse_caller");
        let context = evm.ctx();
        let basefee = context.block().basefee() as u128;
        let caller = context.tx().caller();
        let effective_gas_price = context.tx().effective_gas_price(basefee);
        let gas = exec_result.gas();

        let reimbursement =
            effective_gas_price.saturating_mul((gas.remaining() + gas.refunded() as u64) as u128);
        gas_token_operation::<EVM::Context, ERROR>(
            context,
            TREASURY,
            caller,
            U256::from(reimbursement),
        )?;

        Ok(())
    }

    /// Rewards the beneficiary (miner/validator) with gas fees
    fn reward_beneficiary(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <Self::Frame as Frame>::FrameResult,
    ) -> Result<(), Self::Error> {
        let context = evm.ctx();
        let tx = context.tx();
        let beneficiary = context.block().beneficiary();
        let basefee = context.block().basefee() as u128;
        let effective_gas_price = tx.effective_gas_price(basefee);
        let gas = exec_result.gas();

        let coinbase_gas_price = if context
            .cfg()
            .spec()
            .is_enabled_in(revm::primitives::hardfork::SpecId::LONDON.into())
        {
            effective_gas_price.saturating_sub(basefee)
        } else {
            effective_gas_price
        };

        let reward =
            coinbase_gas_price.saturating_mul((gas.spent() - gas.refunded() as u64) as u128);
        gas_token_operation::<EVM::Context, ERROR>(
            context,
            TREASURY,
            beneficiary,
            U256::from(reward),
        )?;

        Ok(())
    }

    /// Processes the final execution output.
    ///
    /// This method, retrieves the final state from the journal, converts internal results to the external output format.
    /// Internal state is cleared and EVM is prepared for the next transaction.
    ///
    /// Seismic Addendum
    /// Given that we can't yet pass instruction_result which aren't in the InstructionResult enum,
    /// We leverage context_error to bubble up our instruction set specific errors! We also clear
    /// the rng state on returns that won't go through catch_error.
    #[inline]
    fn output(
        &self,
        evm: &mut Self::Evm,
        result: <Self::Frame as Frame>::FrameResult,
    ) -> Result<ResultAndState<Self::HaltReason>, Self::Error> {
        match core::mem::replace(evm.ctx().error(), Ok(())) {
            Err(ContextError::Db(e)) => Err(e.into()),
            Err(ContextError::Custom(e)) => {
                if let Some(seismic_reason) =
                    SeismicHaltReason::try_from_error_string(&e.to_string())
                {
                    let state = evm.ctx().journal().finalize().state;
                    evm.ctx().journal().clear();
                    evm.ctx().chain().reset_rng();

                    return Ok(ResultAndState {
                        result: ExecutionResult::Halt {
                            reason: seismic_reason,
                            gas_used: evm.ctx().tx().gas_limit(),
                        },
                        state,
                    });
                }

                Err(Self::Error::from_string(e))
            }
            Ok(_) => {
                let output = post_execution::output(evm.ctx(), result);
                evm.ctx().journal().clear();
                evm.ctx().chain().reset_rng();
                Ok(output)
            }
        }
    }

    /// Handles cleanup when an error occurs during execution.
    ///
    /// Ensures the journal state is properly cleared before propagating the error.
    /// Also ensures the rng has been reset.
    /// On happy path journal is cleared in [`Handler::output`] method.
    #[inline]
    fn catch_error(
        &self,
        evm: &mut Self::Evm,
        error: Self::Error,
    ) -> Result<ResultAndState<Self::HaltReason>, Self::Error> {
        // Clean up journal state if error occurs
        evm.ctx().journal().clear();
        evm.ctx().chain().reset_rng();
        Err(error)
    }
}

// Fix for the first error: Simplify the InspectorHandler implementation with proper bounds
impl<EVM, ERROR, FRAME> InspectorHandler for SeismicHandler<EVM, ERROR, FRAME>
where
    EVM: InspectorEvmTr<Context: SeismicContextTr>,
    ERROR: EvmTrError<EVM> + From<InvalidTransaction> + FromStringError,
    FRAME: InspectorFrame<
        Evm = EVM,
        Error = ERROR,
        FrameResult = FrameResult,
        FrameInit = FrameInput,
        IT = EthInterpreter,
    >,
    EVM::Inspector: Inspector<EVM::Context, EthInterpreter>,
{
    type IT = EthInterpreter;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::default_ctx::{DefaultSeismicContext, DefaultSeismicDB, SeismicContext},
        SeismicBuilder,
    };
    use revm::{
        context::{result::EVMError, Context},
        handler::EthFrame,
        interpreter::{CallOutcome, Gas, InstructionResult, InterpreterResult},
        primitives::Bytes,
    };

    /// Creates frame result.
    fn call_last_frame_return(
        ctx: SeismicContext<DefaultSeismicDB>,
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
            SeismicHandler::<_, EVMError<_, InvalidTransaction>, EthFrame<_, _, _>>::new();

        handler
            .last_frame_result(&mut evm, &mut exec_result)
            .unwrap();
        handler.refund(&mut evm, &mut exec_result, 0);
        *exec_result.gas()
    }

    #[test]
    fn test_revert_gas() {
        let ctx = Context::seismic().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let gas = call_last_frame_return(ctx, InstructionResult::Revert, Gas::new(90));
        assert_eq!(gas.remaining(), 90);
        assert_eq!(gas.spent(), 10);
        assert_eq!(gas.refunded(), 0);
    }

    #[test]
    fn test_fatal_external_error_gas() {
        let ctx = Context::seismic().modify_tx_chained(|tx| {
            tx.base.gas_limit = 100;
        });

        let gas = call_last_frame_return(ctx, InstructionResult::FatalExternalError, Gas::new(90));
        assert_eq!(gas.remaining(), 0);
        assert_eq!(gas.spent(), 100);
        assert_eq!(gas.refunded(), 0);
    }
}
