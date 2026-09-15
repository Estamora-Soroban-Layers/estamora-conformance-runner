//! What was observed about one contract call.
//!
//! These are plain data structures rather than the execution layer's types, and
//! that is a decision with consequences. The evaluator never sees an `Env`, so a
//! dimension can be pinned by a test that constructs the observation it wants
//! instead of a contract that produces it — and the crate stays free of the SDK, so
//! the whole assertion layer compiles and runs without an execution environment.
//!
//! # The distinction this file exists to keep
//!
//! A call ends in one of three ways: the contract returned, the contract refused,
//! or the environment could not produce an observation. Only the first two say
//! anything about the contract. So [`CallResult::CouldNotRun`] is a separate variant
//! rather than a flavour of refusal, and every evaluator that reads it produces no
//! verdict rather than a failing one.
//!
//! [`CallResult::Refused`] carries whether the interface had been checked first,
//! because Soroban reports a deliberate `panic!()` and a call to a method that does
//! not exist through the same channel. Without that bit, a missing method would be
//! indistinguishable from a working authorization check.

use crate::value::Value;

/// How a call ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallResult {
    /// The contract returned a value.
    Returned,
    /// The contract refused the call.
    Refused {
        /// The contract's own error code, when it reported one through a typed
        /// contract error.
        ///
        /// `None` for a bare `panic!()`, which is the dominant idiom and is a
        /// refusal rather than an environment failure.
        code: Option<u32>,
    },
    /// The call could not be evaluated, so no statement about the contract follows
    /// from it.
    CouldNotRun {
        /// What the environment reported.
        detail: String,
    },
}

/// One event a call emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedEvent {
    /// The event's first topic read as a name, when it is a symbol.
    ///
    /// `None` rather than an error when the first topic is not a symbol: an event
    /// whose name is not a symbol simply does not match a name-based requirement,
    /// and its topics remain available to a requirement that matches positionally.
    pub name: Option<String>,
    /// Every topic, in order.
    pub topics: Vec<Value>,
    /// The payload.
    pub data: Value,
}

/// One authorization a call required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedAuthorization {
    /// The account or contract that was asked to authorize, named as the fixture
    /// actor the vector declared.
    pub actor: String,
    /// The argument names the authorization was observed to cover.
    ///
    /// Derived by comparing the authorized address against the call's own argument
    /// values, so it is what the contract *demanded* rather than what was offered.
    /// This is the field that makes argument coverage a checkable requirement
    /// instead of an aspiration.
    pub covers: Vec<String>,
}

/// Everything observed about one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedCall {
    /// The method that was called.
    pub method: String,
    /// How the call ended.
    pub outcome: CallResult,
    /// The value returned, when one was.
    pub returned: Option<Value>,
    /// The events emitted, in emission order.
    pub events: Vec<ObservedEvent>,
    /// The authorizations the call required, in the order they were recorded.
    pub authorizations: Vec<ObservedAuthorization>,
    /// Whether the interface was inspected before this call.
    ///
    /// The control that keeps a refusal honest: a `false` here means an abort
    /// reported from this observation must not be read as a contract decision.
    pub interface_verified: bool,
}

impl ObservedCall {
    /// A call that returned a value, with nothing else observed.
    ///
    /// A constructor rather than struct literal syntax at every call site, because
    /// a test that forgot `interface_verified` would be a test that could not tell
    /// a refusal from a missing method.
    #[must_use]
    pub fn returned(method: &str, value: Option<Value>, interface_verified: bool) -> Self {
        Self {
            method: method.to_owned(),
            outcome: CallResult::Returned,
            returned: value,
            events: Vec::new(),
            authorizations: Vec::new(),
            interface_verified,
        }
    }

    /// Whether the contract refused the call.
    #[must_use]
    pub const fn refused(&self) -> bool {
        matches!(self.outcome, CallResult::Refused { .. })
    }

    /// Whether the call completed, so its effects count.
    #[must_use]
    pub const fn accepted(&self) -> bool {
        matches!(self.outcome, CallResult::Returned)
    }

    /// Whether a refusal can be read as a deliberate one.
    ///
    /// `true` only when the interface was verified first. An evaluator that treats a
    /// refusal as satisfying "must fail" has to consult this, and
    /// [`crate::failures`] refuses to count a refusal when it is `false`.
    #[must_use]
    pub const fn refusal_is_unambiguous(&self) -> bool {
        self.refused() && self.interface_verified
    }

    /// The events whose name is `name`.
    #[must_use]
    pub fn events_named(&self, name: &str) -> Vec<&ObservedEvent> {
        self.events
            .iter()
            .filter(|event| event.name.as_deref() == Some(name))
            .collect()
    }

    /// Every authorization that was recorded, as `(actor, covered arguments)`.
    #[must_use]
    pub fn authorizing_actors(&self) -> Vec<&str> {
        self.authorizations
            .iter()
            .map(|authorization| authorization.actor.as_str())
            .collect()
    }

    /// The argument names any recorded authorization covered.
    #[must_use]
    pub fn covered_arguments(&self) -> Vec<String> {
        let mut covered: Vec<String> = self
            .authorizations
            .iter()
            .flat_map(|authorization| authorization.covers.iter().cloned())
            .collect();
        covered.sort();
        covered.dedup();
        covered
    }
}
