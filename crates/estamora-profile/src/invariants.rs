//! The `invariants.yaml` document.
//!
//! An invariant is a property that must survive an operation, stated once and
//! reusable across vectors. It is deliberately not a comment: "balances are
//! conserved" is a claim the runner evaluates, and a claim it can fail.
//!
//! # Why every invariant is scoped
//!
//! Conservation is true of a transfer and false of a mint, so an unscoped
//! invariant would be wrong for at least one operation in the profile. The scope
//! is therefore mandatory and states which methods and which outcomes the
//! property is claimed for. Applying an invariant outside its scope is the
//! failure mode this field exists to prevent, and the runner treats an
//! out-of-scope invariant as inapplicable rather than as satisfied.
//!
//! # The six check families
//!
//! [`InvariantKind`] names how the property is evaluated rather than what it
//! says, because the *how* decides what the runner has to observe:
//!
//! | Kind | What the runner must observe |
//! | ---- | ---------------------------- |
//! | `conservation` | An aggregate over a resource set, before and after |
//! | `state_unchanged` | Every member of the resource set, before and after |
//! | `authorization_blocks_mutation` | The same, for a call that was refused |
//! | `monotonic` | The direction an aggregate moved |
//! | `bounds` | Each member of the set, against a predicate |
//! | `predicate` | Whatever the supplied predicate names |
//!
//! # Severity
//!
//! [`InvariantSeverity::Warning`] exists for properties an upstream document
//! states as SHOULD rather than MUST. A warning is recorded with the result and
//! never turns a passing run into a failing one, which keeps the runner from
//! inventing a stricter standard than the one it claims to encode.

use estamora_core::expr::Predicate;
use serde::Deserialize;

/// The `invariants.yaml` document.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvariantsDocument {
    /// The invariants.
    pub invariants: Vec<InvariantDefinition>,
}

/// One invariant.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvariantDefinition {
    /// Stable identifier, referenced by vectors and behavioural rules.
    pub id: String,
    /// A human title.
    pub title: String,
    /// How the property is evaluated.
    pub kind: InvariantKind,
    /// Whether violating it makes a contract non-conformant.
    pub severity: InvariantSeverity,
    /// One sentence stating the property.
    pub summary: String,
    /// The full statement.
    #[serde(default)]
    pub description: Option<String>,
    /// Where the property is claimed to hold.
    pub scope: InvariantScope,
    /// The resource set the property ranges over.
    ///
    /// Required for the aggregate kinds, and forbidden for the others, because an
    /// aggregate that named no set would have nothing to aggregate.
    #[serde(default)]
    pub resource: Option<String>,
    /// The condition asserted.
    ///
    /// Required for `predicate` and `bounds`; the check families that are defined
    /// by their own name must not also carry a predicate, so that there is one
    /// place to read what the invariant means.
    #[serde(default)]
    pub predicate: Option<Predicate>,
    /// The direction an aggregate must move.
    ///
    /// Required for `monotonic` and forbidden otherwise.
    #[serde(default)]
    pub direction: Option<MonotonicDirection>,
    /// Why the invariant holds and what breaks if it does not.
    ///
    /// Required, because an unexplained invariant cannot be reviewed, and an
    /// unreviewable requirement does not belong in a normative document.
    pub rationale: String,
    /// The upstream passages the invariant was drawn from.
    #[serde(default)]
    pub references: Vec<String>,
}

/// How an invariant is evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvariantKind {
    /// An aggregate across a resource set is unchanged.
    Conservation,
    /// No member of the resource set changed.
    StateUnchanged,
    /// A rejected caller cannot have changed protected state.
    AuthorizationBlocksMutation,
    /// An aggregate moved in one direction only.
    Monotonic,
    /// Every member of the resource set stays within limits.
    Bounds,
    /// Whatever the supplied predicate names.
    Predicate,
}

impl InvariantKind {
    /// The stable machine-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Conservation => "conservation",
            Self::StateUnchanged => "state_unchanged",
            Self::AuthorizationBlocksMutation => "authorization_blocks_mutation",
            Self::Monotonic => "monotonic",
            Self::Bounds => "bounds",
            Self::Predicate => "predicate",
        }
    }

    /// Whether the family ranges over a named resource set.
    ///
    /// `state_unchanged` and `authorization_blocks_mutation` deliberately do
    /// *not*: they claim that **nothing at all** changed, which is a stronger and
    /// different statement from "this set was unchanged". Naming a resource set
    /// on one of them would narrow the claim while reading as though it widened
    /// it, so the format forbids the field rather than ignoring it.
    #[must_use]
    pub const fn ranges_over_resource(self) -> bool {
        matches!(self, Self::Conservation | Self::Monotonic | Self::Bounds)
    }
}

/// Whether violating an invariant makes a contract non-conformant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvariantSeverity {
    /// Violating it is a conformance failure.
    Error,
    /// Violating it is recorded and does not decide the run.
    Warning,
}

/// Where an invariant must hold.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvariantScope {
    /// Method ids the invariant applies to, or `*` for every method.
    pub methods: Vec<String>,
    /// Outcomes the invariant is claimed for.
    pub outcomes: Vec<InvariantOutcome>,
}

/// An outcome an invariant can be scoped to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvariantOutcome {
    /// The operation succeeded.
    Success,
    /// The operation was refused.
    Failure,
}

/// An allowed direction of aggregate movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonotonicDirection {
    /// The aggregate may fall or stay level, and must not rise.
    NonIncreasing,
    /// The aggregate may rise or stay level, and must not fall.
    NonDecreasing,
}
