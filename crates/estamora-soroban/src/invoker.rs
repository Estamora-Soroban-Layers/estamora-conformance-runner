//! Invocation, and the classification of what came back.
//!
//! # The ambiguity this module refuses to guess at
//!
//! The Soroban host reports two very different situations through the same
//! channel. A contract that calls `panic!()` to reject a call, and a call to a
//! method that does not exist, both surface as an `Abort`. One is a contract
//! behaving correctly — rejecting an over-balance transfer is *required* — and
//! the other is an interface that is not what it claimed to be.
//!
//! The runner resolves this by ordering rather than by guessing: interface
//! inspection runs before the behavioural vectors, so by the time a behavioural
//! vector observes an `Abort`, the method is known to exist and the abort can be
//! read as a refusal. [`CallOutcome::Refused`] therefore records the refusal and
//! [`Invocation::abort_is_unambiguous`] records whether that ordering held. A run
//! that invoked a method without inspecting the interface first gets an
//! observation that says so, instead of a silent misclassification that would
//! make a missing method look like a working authorization check.

use estamora_core::{ErrorClass, Result};
use soroban_sdk::{Address, Env, InvokeError, Symbol, Val, Vec};

use crate::auth::AuthorizationRecord;
use crate::events::CapturedEvent;

/// How a call ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallOutcome {
    /// The contract returned a value.
    Returned,
    /// The contract refused the call.
    ///
    /// `code` is the contract's own error code when it reported one through a
    /// typed contract error, and `None` when it aborted without one — a bare
    /// `panic!()`, which is the dominant idiom and is a refusal rather than an
    /// environment failure.
    ///
    /// Whether this refusal was *required*, *permitted* or *forbidden* is a
    /// question for the assertion layer; this module records only that it
    /// happened.
    Refused {
        /// The contract's error code, when it reported one.
        code: Option<u32>,
    },
    /// The call could not be evaluated, so no statement about the contract can be
    /// made from it.
    ///
    /// Distinguished from [`CallOutcome::Refused`] because the two lead to
    /// opposite report statuses: a refusal is evidence about behaviour, and an
    /// evaluation failure is evidence about the environment.
    Failed {
        /// The classification, always an environment class rather than a
        /// contract one.
        class: ErrorClass,
        /// What the host reported.
        detail: String,
    },
}

/// Everything observed about one contract call.
#[derive(Debug, Clone)]
pub struct Invocation {
    /// The contract that was called.
    pub contract: Address,
    /// The method that was called.
    pub method: Symbol,
    /// How the call ended.
    pub outcome: CallOutcome,
    /// The value the contract returned, when it returned one.
    pub returned: Option<Val>,
    /// The events emitted by the call, in emission order.
    pub events: std::vec::Vec<CapturedEvent>,
    /// The authorizations the call required, in the order the host recorded them.
    pub authorizations: std::vec::Vec<AuthorizationRecord>,
    /// Whether the interface had been inspected before this call.
    ///
    /// The control that keeps [`CallOutcome::Refused`] honest: without it, an
    /// abort from a missing method is indistinguishable from a deliberate
    /// refusal. The runner sets this only after a successful interface check, so
    /// a `false` here means a refusal reported from this observation must not be
    /// read as a contract decision.
    pub interface_verified: bool,
}

impl Invocation {
    /// Whether a refused call can be read as a deliberate refusal.
    ///
    /// `true` only when the interface was verified first. A profile requirement
    /// that treats a refusal as satisfying "must fail" has to consult this, and
    /// the assertion layer refuses to count a refusal as satisfying anything when
    /// it is `false`.
    #[must_use]
    pub const fn abort_is_unambiguous(&self) -> bool {
        self.interface_verified
    }

    /// Whether the call completed and its effects count.
    #[must_use]
    pub const fn accepted(&self) -> bool {
        matches!(self.outcome, CallOutcome::Returned)
    }

    /// Whether the contract refused the call.
    #[must_use]
    pub const fn refused(&self) -> bool {
        matches!(self.outcome, CallOutcome::Refused { .. })
    }

    /// The number of times `name` appears as an event's first topic.
    ///
    /// Used by cardinality requirements: a transfer that emits two transfer
    /// events is a conformance failure even when both carry the right values,
    /// because a consumer cannot tell which movement is real.
    #[must_use]
    pub fn event_count(&self, env: &Env, name: &str) -> usize {
        let wanted = Symbol::new(env, name);
        self.events
            .iter()
            .filter(|event| event.name(env).as_ref() == Some(&wanted))
            .count()
    }
}

/// Calls `method` on `contract` with `args` and captures everything observable.
///
/// This function never returns an error for a contract that misbehaved, because
/// misbehaviour is an observation and not a failure of the runner. It returns an
/// error only when the *observation itself* cannot be assembled — most
/// importantly, when events cannot be read — which is an environment failure
/// that must not be reported as anything else.
///
/// `interface_verified` states whether the caller has already confirmed that
/// `method` exists on `contract`. See [`Invocation::abort_is_unambiguous`].
///
/// # Errors
///
/// Returns an execution failure if the emitted events cannot be converted into
/// the runner's representation. A partial capture would let an event requirement
/// be reported as satisfied on incomplete evidence.
pub fn invoke(
    env: &Env,
    contract: &Address,
    method: &str,
    args: Vec<Val>,
    interface_verified: bool,
) -> Result<Invocation> {
    let symbol = Symbol::new(env, method);
    let attempted = env.try_invoke_contract::<Val, InvokeError>(contract, &symbol, args);

    // Capture before interpreting: the events and authorizations belong to the
    // call regardless of how it ended, and a failure to read them is a failure of
    // the observation rather than of the contract.
    let events = CapturedEvent::capture(env)?;
    let authorizations = AuthorizationRecord::recorded(env);

    let (outcome, returned) = match attempted {
        Ok(Ok(value)) => (CallOutcome::Returned, Some(value)),
        // The value came back in a shape that could not be read as the requested
        // type. The contract returned something, and the runner cannot interpret
        // it, so it has no observation to offer.
        Ok(Err(reported)) => (
            CallOutcome::Failed {
                class: ErrorClass::ExecutionError,
                detail: format!("the returned value could not be read: {reported:?}"),
            },
            None,
        ),
        Err(Ok(InvokeError::Contract(code))) => (CallOutcome::Refused { code: Some(code) }, None),
        Err(Ok(InvokeError::Abort)) => (CallOutcome::Refused { code: None }, None),
        // The invocation itself failed: the function does not exist, the
        // arguments did not match its signature, or the host ran out of budget.
        // None of these is a statement about the contract's behaviour, so the
        // class is an environment one. The interface layer is what distinguishes
        // a missing function from a host failure; this observation cannot.
        Err(Err(problem)) => (
            CallOutcome::Failed {
                class: ErrorClass::ExecutionError,
                detail: format!("{problem:?}"),
            },
            None,
        ),
    };

    Ok(Invocation {
        contract: contract.clone(),
        method: symbol,
        outcome,
        returned,
        events,
        authorizations,
        interface_verified,
    })
}
