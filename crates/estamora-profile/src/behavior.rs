//! The `behavior.yaml` document.
//!
//! This is where behavioural conformance is actually stated. Each rule is one
//! operation's contract with its caller: the preconditions under which it
//! applies, the postconditions that must hold after it, the failures it is
//! allowed to produce, the events it must and must not emit, and the invariants
//! that must survive it.
//!
//! # Why preconditions are checked before the operation runs
//!
//! A precondition is not an assertion about the contract; it is a statement that
//! *this scenario is the one the rule describes*. If a fixture were fed a rule
//! whose precondition did not hold, the postconditions would be evaluated against
//! a world the rule never claimed anything about, and a passing result would mean
//! nothing. So a precondition that does not hold makes the rule **inapplicable**
//! rather than failed, which is recorded as a skip and reported as such.
//!
//! # Why success and failure are separate rules
//!
//! A profile that only described successful behaviour would accept a contract
//! that silently succeeds where the standard requires a rejection. That is the
//! most dangerous kind of non-conformance, because it is invisible to every happy
//! path test and it is exactly what the failure rules exist to catch.

use estamora_core::expr::Predicate;
use serde::Deserialize;

/// The `behavior.yaml` document.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorDocument {
    /// The behavioural rules.
    pub behaviors: Vec<BehaviorRule>,
}

/// One behavioural rule.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorRule {
    /// Stable identifier, referenced by method requirements.
    pub id: String,
    /// One sentence stating the requirement.
    pub summary: String,
    /// The full statement.
    #[serde(default)]
    pub description: Option<String>,
    /// Id of the method this rule governs.
    ///
    /// A rule is scoped to one method rather than to several because a rule that
    /// claimed to hold for a set of methods would be a rule whose failure could
    /// not be attributed to any of them.
    pub method: String,
    /// Whether the rule describes a path that must succeed or one that must fail.
    pub kind: BehaviorKind,
    /// Conditions that must hold for the rule to apply.
    ///
    /// Evaluated against the fixture before the operation runs. A precondition
    /// that does not hold makes the rule inapplicable, not failed.
    pub preconditions: Vec<Predicate>,
    /// Conditions that must hold after the operation, when the rule applies.
    pub postconditions: Vec<Predicate>,
    /// Ids of failures the operation must produce, when the rule is a failure
    /// rule.
    pub expect_failures: Vec<String>,
    /// Ids of events the operation must emit.
    pub expect_events: Vec<String>,
    /// Ids of events the operation must not emit.
    ///
    /// What makes "a refused call emits nothing" enforceable: a contract that
    /// emitted its event before checking its balance would otherwise look
    /// conformant, and it is precisely the contract the requirement exists to
    /// catch.
    pub forbid_events: Vec<String>,
    /// Ids of invariants that must survive the operation.
    pub invariants: Vec<String>,
    /// Anything a reader should know that is not a requirement.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Which path a rule describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorKind {
    /// A path that must succeed.
    Success,
    /// A path that must fail.
    Failure,
}

impl BehaviorKind {
    /// The stable machine-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }
}
