//! The vector document, as typed structures.
//!
//! A vector is a first-class specification artifact, not a test fixture in a
//! runner-specific format. It describes a deterministic world, one operation
//! against it, and exactly what must be observed — and it has to be *declarative*
//! because two independent implementations, in different languages, must be able
//! to execute the same vector and reach the same verdict.
//!
//! # Why amounts are strings
//!
//! Every quantity is written as a string and parsed into an `i128`
//! ([`IntegerString`]). An `i128` does not survive a round trip through a
//! double-precision float, and a JSON parser that decided `250` was a number would
//! silently round an expected balance — which would weaken every assertion that
//! reads it, invisibly, for exactly the large values where it matters most.
//!
//! # Why the same shapes appear in profiles and vectors
//!
//! A vector's expectations are written in the same expression algebra a profile's
//! `behavior.yaml` uses, which lives in `estamora-core` so that there is one
//! definition of what `equal` means. What a vector adds is the *world*: the
//! fixture actors, the opening balances, the ledger point, and the inputs a
//! specific call is made with.
//!
//! # Rejecting unknown fields
//!
//! Every structure here rejects an unknown field. A field the runner silently
//! ignores is a requirement that was written, reviewed and then never enforced,
//! and the only acceptable outcome is a deserialize failure that names it.

use std::collections::BTreeMap;

use estamora_core::expr::{Predicate, ValueExpr};
use estamora_profile::failures::FailureCategory;
use serde::Deserialize;

use crate::integer::IntegerString;

/// The `*.yaml` vector document.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorDocument {
    /// The schema the document is written against, when it declares one.
    ///
    /// Accepted and ignored: the specification's own documents name the schema in
    /// an editor directive rather than a field, but the schema permits it, so a
    /// document that carries it is not a document to refuse.
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    /// Globally unique vector identifier.
    pub id: String,
    /// The profile this vector belongs to, or `*` for a profile-independent one.
    pub profile: String,
    /// The profile version the vector was written against.
    ///
    /// For a wildcard vector this is the Estamora format version, because a
    /// vector that belongs to no profile cannot be pinned to one's revision.
    pub profile_version: String,
    /// A human title.
    pub title: String,
    /// What the vector establishes.
    ///
    /// Required, because a vector whose purpose is not written down becomes
    /// unmaintainable the moment its original author moves on.
    pub description: String,
    /// The class of property the vector probes.
    pub kind: VectorKind,
    /// Id of the method under test.
    pub method: String,
    /// Free-form labels used to select a subset of a suite in CI.
    pub tags: Vec<String>,
    /// The deterministic world the operation runs against.
    pub fixtures: Fixtures,
    /// Argument name to value.
    pub inputs: BTreeMap<String, ValueExpr>,
    /// Which actors sign the invocation, and what that is expected to achieve.
    pub authorization: AuthorizationPlan,
    /// What must be observed.
    pub expected: ExpectedOutcome,
    /// Additional independently reported checks.
    ///
    /// Reported individually rather than collapsed into one boolean, so that a
    /// failure names the exact rule that was violated.
    pub assertions: Vec<Assertion>,
    /// Why this vector exists and which requirement it protects.
    pub rationale: String,
    /// The upstream passages the vector was drawn from.
    pub references: Vec<String>,
}

/// The class of property a vector probes.
///
/// Every class is mandatory for a complete profile. A suite of only positive
/// vectors cannot establish behavioural conformance, because the defects that
/// matter most are the ones that only appear when a call is supposed to fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorKind {
    /// A path that must succeed.
    Positive,
    /// A path that must fail.
    Negative,
    /// A boundary of the interface's domain.
    Boundary,
    /// An authorization requirement.
    Authorization,
    /// An event requirement.
    Event,
    /// A state transition.
    State,
    /// An invariant.
    Invariant,
}

impl VectorKind {
    /// The stable machine-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Positive => "positive",
            Self::Negative => "negative",
            Self::Boundary => "boundary",
            Self::Authorization => "authorization",
            Self::Event => "event",
            Self::State => "state",
            Self::Invariant => "invariant",
        }
    }
}

/// The deterministic world an operation runs against.
///
/// Every input must be stated, because a vector that depends on ambient state
/// cannot be reproduced on another machine — and a result nobody else can
/// reproduce is not evidence.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixtures {
    /// The accounts and contracts the operation names.
    pub actors: Vec<FixtureActor>,
    /// Opening balance per fixture actor name.
    pub balances: BTreeMap<String, IntegerString>,
    /// Allowances granted before the operation runs.
    pub allowances: Vec<FixtureAllowance>,
    /// The opening total supply, where the interface exposes one.
    #[serde(default)]
    pub total_supply: Option<IntegerString>,
    /// The ledger point the operation executes at.
    pub ledger: LedgerFixture,
    /// Whether each actor holds token-level authorization.
    ///
    /// Distinct from transaction authorization: a contract may be authorized to
    /// move value and still fail its own authorization flag.
    pub authorization: BTreeMap<String, bool>,
}

/// One account or contract in a fixture.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureActor {
    /// The name the operation refers to it by.
    pub name: String,
    /// Whether it is an account or a contract.
    pub kind: ActorKind,
    /// A link into the shared fixture library.
    ///
    /// So that the same address is used across profiles instead of being invented
    /// once per vector, which is what makes two profiles' results comparable.
    #[serde(rename = "ref", default)]
    pub reference: Option<String>,
    /// What the actor is for.
    #[serde(default)]
    pub description: Option<String>,
}

/// Whether a fixture actor is an account or a contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    /// A Stellar account.
    Account,
    /// A deployed contract.
    Contract,
}

/// An allowance granted before the operation runs.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureAllowance {
    /// The holder granting it.
    pub from: String,
    /// The account receiving it.
    pub spender: String,
    /// The amount granted.
    pub amount: IntegerString,
    /// The ledger at which it expires.
    ///
    /// Omitted when the fixture leaves expiry to the implementation's default.
    /// Present when the vector is about expiry, because expiry cannot be observed
    /// through the interface once it has lapsed.
    #[serde(default)]
    pub live_until_ledger: Option<u32>,
}

/// The ledger point an operation executes at.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerFixture {
    /// The ledger sequence number.
    pub sequence: u32,
    /// The ledger close time, as an RFC 3339 timestamp.
    pub timestamp: String,
}

impl LedgerFixture {
    /// The close time as seconds since the Unix epoch.
    ///
    /// # Errors
    ///
    /// Returns a vector error if the timestamp is not an RFC 3339 date-time, or
    /// if it is earlier than the epoch. Both are document defects rather than
    /// contract findings: the runner cannot construct the declared world, so it
    /// has nothing to measure.
    pub fn timestamp_unix(&self) -> estamora_core::Result<u64> {
        let parsed = time::OffsetDateTime::parse(
            &self.timestamp,
            &time::format_description::well_known::Rfc3339,
        )
        .map_err(|problem| {
            estamora_core::Error::new(
                estamora_core::ErrorClass::VectorError,
                format!(
                    "the ledger timestamp {:?} is not an RFC 3339 date-time: {problem}",
                    self.timestamp
                ),
            )
        })?;
        u64::try_from(parsed.unix_timestamp()).map_err(|_| {
            estamora_core::Error::new(
                estamora_core::ErrorClass::VectorError,
                format!(
                    "the ledger timestamp {:?} is before the Unix epoch",
                    self.timestamp
                ),
            )
        })
    }
}

/// Which actors sign the invocation, and what that is expected to achieve.
///
/// Modelled explicitly so that authorized success, unauthorized failure and
/// wrong-actor failure are three separately testable outcomes rather than one.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationPlan {
    /// The actors that sign.
    pub actors: Vec<String>,
    /// What the signing is expected to achieve.
    pub expected: AuthorizationExpectation,
    /// Actors whose authorization is deliberately replaced by another actor.
    ///
    /// This is how a wrong-actor case is constructed: the call must be authorized
    /// by *somebody*, just not by the principal the profile requires.
    #[serde(default)]
    pub substituted_for: Vec<String>,
}

/// What a call's signing is expected to achieve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationExpectation {
    /// The call must be accepted.
    Accepted,
    /// The call must be refused.
    Rejected,
    /// The call must be accepted whether or not anybody signs.
    NotRequired,
}

/// What must be observed.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedOutcome {
    /// Whether the call must succeed or fail.
    pub outcome: ExpectedResult,
    /// Id of the failure requirement this vector must produce.
    ///
    /// Required when the outcome is `failure`, so that a vector can never merely
    /// assert that something went wrong.
    #[serde(default)]
    pub failure: Option<String>,
    /// The semantic gap this vector fills.
    ///
    /// Used instead of `failure` by a profile-independent vector, which belongs to
    /// no bundle and therefore cannot name a profile's own failure definition — but
    /// the failure categories are Estamora-level vocabulary every profile shares,
    /// so a wildcard vector can still say precisely why a call must fail.
    #[serde(default)]
    pub failure_category: Option<FailureCategory>,
    /// The expected return value. Omitted for a void return.
    #[serde(default)]
    pub returns: Option<ValueExpr>,
    /// The state that must hold after the operation.
    pub state_assertions: Vec<StateAssertion>,
    /// The events that must and must not appear.
    pub events: EventExpectations,
    /// Ids of the invariants that must survive the operation.
    pub invariants: Vec<String>,
}

/// Whether a call must succeed or fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedResult {
    /// The call must be accepted.
    Success,
    /// The call must be refused.
    Failure,
}

/// The events a vector requires and forbids.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventExpectations {
    /// The events that must appear.
    pub required: Vec<EventExpectation>,
    /// Ids of events that must not appear.
    ///
    /// What makes "a refused call emits nothing" enforceable: a contract that
    /// emitted its event before checking its balance would otherwise look
    /// conformant, and it is precisely the contract the requirement exists to
    /// catch.
    pub forbidden: Vec<String>,
}

/// A concrete expected event, matched against the observation log.
///
/// Values are expressions rather than literals so that an expectation can refer to
/// the actual transfer amount instead of restating it, which is what keeps the
/// vector valid when a fixture changes.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventExpectation {
    /// Id of the event requirement being matched.
    pub event: String,
    /// The expected topics, in position order.
    pub topics: Vec<ValueExpr>,
    /// The expected payload, in declaration order.
    pub data: Vec<ValueExpr>,
}

/// One state assertion.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateAssertion {
    /// Stable identifier, reported verbatim when the assertion fails.
    pub id: String,
    /// What the assertion establishes.
    #[serde(default)]
    pub description: Option<String>,
    /// The piece of contract state the assertion reads.
    pub resource: StateResourceRef,
    /// The condition that must hold.
    pub predicate: Predicate,
}

/// Identifies the piece of contract state an assertion reads.
///
/// Components of the operation that define a balance or an allowance are named, so
/// that a state assertion can be checked without the runner guessing which account
/// it refers to. A guess would be a silent misattribution: the right answer for
/// the wrong account.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StateResourceRef {
    /// One actor's balance.
    Balance {
        /// The actor whose balance is read.
        account: String,
    },
    /// One allowance.
    Allowance {
        /// The holder granting it.
        from: String,
        /// The account receiving it.
        spender: String,
    },
    /// The whole balance set.
    TotalSupply,
    /// The ledger sequence the operation ran at.
    LedgerSequence,
    /// A resource the profile names and describes how to read.
    Custom {
        /// The resource's name, declared by the profile that consumes the vector.
        name: String,
        /// How to read it.
        read: ValueExpr,
    },
}

/// The conformance dimension an assertion belongs to.
///
/// Categories are reported separately so that a passing interface check can never
/// be presented as evidence of correct behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionCategory {
    /// The interface's shape.
    Interface,
    /// Authorization.
    Authorization,
    /// Events.
    Event,
    /// Behaviour.
    Behavior,
    /// Contract state.
    State,
    /// A property that must survive the operation.
    Invariant,
    /// The reason a call failed.
    Failure,
}

impl AssertionCategory {
    /// The stable machine-readable name, matching the report schema.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Interface => "interface",
            Self::Authorization => "authorization",
            Self::Event => "event",
            Self::Behavior => "behavior",
            Self::State => "state",
            Self::Invariant => "invariant",
            Self::Failure => "failure",
        }
    }

    /// Every category, in the order the report schema lists them.
    pub const ALL: [Self; 7] = [
        Self::Interface,
        Self::Authorization,
        Self::Event,
        Self::Behavior,
        Self::State,
        Self::Invariant,
        Self::Failure,
    ];
}

/// One independently reported check.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    /// Stable identifier, reported verbatim so that a failure names the rule.
    pub id: String,
    /// The conformance dimension it belongs to.
    pub category: AssertionCategory,
    /// What it establishes.
    pub description: String,
    /// The condition that must hold.
    pub predicate: Predicate,
}
