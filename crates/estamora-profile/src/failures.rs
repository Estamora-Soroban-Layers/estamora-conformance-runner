//! The `failures.yaml` document.
//!
//! Negative testing is mandatory: a contract that silently succeeds where the
//! standard requires a rejection is the most dangerous kind of non-conformance,
//! because no happy-path test can detect it. So a failure requirement is not a
//! remark that "the call should fail" — it states *why* it must fail, *how* the
//! contract may signal it, and what must be left behind afterwards.
//!
//! # Why the payload is not pinned
//!
//! Soroban signals failure by trapping, and the trap payload is
//! implementation-defined. Pinning an exact error symbol would therefore reject
//! conforming contracts that chose a different one, which is how a specification
//! turns into a compatibility test for one implementation. Estamora standardises
//! the *semantic category* of a failure instead, and lets a profile opt in to
//! constraining the payload only where an upstream document genuinely names it.
//!
//! [`ErrorCodePolicy`] is that opt-in, and `SemanticOnly` is the default. It is
//! not a weaker requirement: a contract that fails for the wrong reason is caught
//! by the method scope and the trigger, not by an error string.
//!
//! # Why the state effect is stated separately from the signal
//!
//! A contract can fail correctly and still have written to storage before its
//! check ran. The signal says the call was refused; [`StateEffect`] says whether
//! the refusal was clean. They are independent, and a profile that stated only
//! the first would accept a contract whose failed calls are indistinguishable
//! from accepted ones to anyone reading storage.

use serde::Deserialize;

/// The `failures.yaml` document.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailuresDocument {
    /// The failure requirements.
    pub failures: Vec<FailureDefinition>,
}

/// One failure requirement.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureDefinition {
    /// Stable identifier, referenced by vectors, authorization rules and
    /// behavioural rules.
    pub id: String,
    /// The semantic class of the failure.
    pub category: FailureCategory,
    /// One sentence stating the requirement.
    pub summary: String,
    /// The full statement.
    #[serde(default)]
    pub description: Option<String>,
    /// Ids of the methods from which this failure is reachable.
    pub methods: Vec<String>,
    /// The condition that must produce the failure, as a precondition a vector
    /// can construct.
    ///
    /// Required because "this call must fail" is not a checkable requirement: a
    /// contract that fails for an unrelated reason would satisfy it, and the
    /// requirement would never catch the defect it exists for.
    pub trigger: String,
    /// What must be observed.
    pub expected: FailureExpectation,
    /// How strictly the failure's payload is constrained.
    pub error_codes: ErrorCodeRequirement,
    /// Why the requirement exists.
    pub rationale: String,
    /// The upstream passages the requirement was drawn from.
    #[serde(default)]
    pub references: Vec<String>,
}

/// The semantic class of a failure.
///
/// A closed registry, so that a report can state a stable category. `Custom` is
/// the single escape hatch for profile-specific cases, and a profile that uses it
/// must still explain the case in prose — the alternative would be a free-form
/// string vocabulary that no consumer could match on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    /// The operation moves more than the source holds.
    InsufficientBalance,
    /// The operation exceeds the remaining allowance.
    InsufficientAllowance,
    /// Authorization was required and refused.
    Unauthorized,
    /// Authorization was required and not supplied at all.
    MissingAuthorization,
    /// A different principal authorized than the one required.
    WrongActor,
    /// The amount cannot describe a valid quantity.
    InvalidAmount,
    /// The address is not one the contract can act on.
    InvalidAddress,
    /// Some other argument is malformed.
    InvalidArgument,
    /// The contract is not in a state where the operation is possible.
    InvalidState,
    /// The contract has not been initialized.
    Uninitialized,
    /// A value that has lapsed was used.
    Expired,
    /// The operation is not supported by this implementation.
    UnsupportedOperation,
    /// Arithmetic would leave the representable range.
    ArithmeticOverflow,
    /// A case specific to this profile.
    Custom,
}

impl FailureCategory {
    /// The stable machine-readable name, matching the specification's vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InsufficientBalance => "insufficient_balance",
            Self::InsufficientAllowance => "insufficient_allowance",
            Self::Unauthorized => "unauthorized",
            Self::MissingAuthorization => "missing_authorization",
            Self::WrongActor => "wrong_actor",
            Self::InvalidAmount => "invalid_amount",
            Self::InvalidAddress => "invalid_address",
            Self::InvalidArgument => "invalid_argument",
            Self::InvalidState => "invalid_state",
            Self::Uninitialized => "uninitialized",
            Self::Expired => "expired",
            Self::UnsupportedOperation => "unsupported_operation",
            Self::ArithmeticOverflow => "arithmetic_overflow",
            Self::Custom => "custom",
        }
    }

    /// Whether the category describes an authorization path.
    ///
    /// Authorization categories are held to a stricter signal rule: a rejection
    /// must come from the contract's own check, so a host-level error would mean
    /// the failure happened for a different reason than the profile claims.
    #[must_use]
    pub const fn is_authorization(self) -> bool {
        matches!(
            self,
            Self::Unauthorized | Self::MissingAuthorization | Self::WrongActor
        )
    }
}

/// What must be observed when the failure occurs.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureExpectation {
    /// Always `failure`. A definition that expects success belongs in
    /// `behavior.yaml`, and the schema rejects the alternative rather than
    /// letting one document quietly do the other's job.
    pub outcome: FailureOutcome,
    /// How the contract may signal the refusal.
    pub signal: FailureSignal,
    /// What must have happened to the protected state.
    pub state_effect: StateEffect,
    /// What must have happened to the event log.
    pub events_emitted: EventsEmitted,
}

/// The outcome a failure definition expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureOutcome {
    /// The call must be refused.
    Failure,
}

/// How a contract may signal a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureSignal {
    /// The contract trapped.
    Trap,
    /// The contract panicked.
    Panic,
    /// The host refused the call before the contract's own check ran.
    HostError,
    /// Any of the above.
    ///
    /// The common case, and not a relaxation: whether the host refuses an
    /// invocation whose authorization payload is absent, or the contract's own
    /// check is reached and traps, depends on where the implementation put the
    /// check, and both are correct. A profile that demanded one would reject a
    /// conforming contract on the basis of where it happens to check.
    Either,
}

impl FailureSignal {
    /// The stable machine-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trap => "trap",
            Self::Panic => "panic",
            Self::HostError => "host_error",
            Self::Either => "either",
        }
    }
}

/// What must have happened to the protected state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateEffect {
    /// The call left no trace: every mutation was rolled back.
    Reverted,
    /// Nothing observable changed, which additionally allows a view-visible
    /// counter to have moved.
    Unchanged,
    /// The upstream text is silent, so nothing is required.
    Unspecified,
}

/// What must have happened to the event log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventsEmitted {
    /// The refused operation emitted nothing.
    ///
    /// The norm, and the requirement that matters most: a failure that emitted
    /// the success event would corrupt every indexer built on the standard.
    None,
    /// The upstream text is silent.
    Unspecified,
}

/// How strictly a failure's payload is constrained.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorCodeRequirement {
    /// The policy.
    pub policy: ErrorCodePolicy,
    /// Payloads the failure tolerates.
    ///
    /// Must be empty under `SemanticOnly` and hold exactly one entry under
    /// `ExactRequired`, which is what keeps the list from becoming a place where
    /// an unconstrained requirement is quietly recorded.
    pub allowed: Vec<String>,
}

/// How a failure's payload is constrained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCodePolicy {
    /// Only the reason matters; the payload may be anything.
    SemanticOnly,
    /// A listed set of payloads is accepted when present.
    Tolerated,
    /// One specific payload is required, and only an upstream document that
    /// names it justifies the requirement.
    ExactRequired,
}
