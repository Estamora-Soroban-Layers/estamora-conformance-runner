//! The `authorization.yaml` document.
//!
//! Authorization is a first-class conformance dimension, so it is modelled here
//! rather than left to the vectors. A rule states *which* principal must
//! authorize, *which* arguments their authorization must cover, and what must
//! happen when it is absent and when it belongs to the wrong party.
//!
//! # The three outcomes a rule distinguishes
//!
//! A naive authorization test checks that a call without a signature fails. That
//! establishes almost nothing: a contract that checks *that* some signature is
//! present, without checking *whose*, passes it and is fully exploitable. The
//! model therefore keeps three cases apart — authorized success, unauthorized
//! failure, and wrong-actor failure — because they exercise different code paths
//! and a contract can get one right while getting another wrong.
//!
//! # Argument coverage
//!
//! A required principal is only half the requirement. SEP-41-style interfaces
//! require a `transfer` to be authorized by the debited account *over the `from`
//! argument specifically*, so [`AuthorizationCoverage`] states how the set of
//! authorization-bearing arguments must relate to the set the contract actually
//! demands. Without it, a contract that requires a signature but never binds it
//! to the account it moves value out of would look conformant.

use serde::Deserialize;

/// The `authorization.yaml` document.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationDocument {
    /// The authorization rules.
    pub authorization_rules: Vec<AuthorizationRule>,
}

/// One authorization requirement.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationRule {
    /// Stable identifier, referenced by the method requirements.
    pub id: String,
    /// One sentence stating the requirement.
    pub summary: String,
    /// The full statement.
    #[serde(default)]
    pub description: Option<String>,
    /// Ids of the methods this rule governs.
    pub methods: Vec<String>,
    /// Which principal must authorize.
    pub actor: AuthorizationActor,
    /// Which arguments their authorization must cover.
    pub coverage: AuthorizationCoverage,
    /// What must happen when no authorization is supplied.
    pub unauthorized: AuthorizationOutcome,
    /// What must happen when the wrong principal authorizes.
    pub wrong_actor: AuthorizationOutcome,
    /// Whether the requirement is sensitive to replay.
    ///
    /// Recorded because a contract that implements its own nonce has a different
    /// conformance surface from one that relies on the host's authorization
    /// framework, and a report should be able to say which was assumed.
    pub replay_sensitive: bool,
    /// Anything a reader should know that is not a requirement.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// The principal a rule requires to authorize.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthorizationActor {
    /// The principal is named by one of the call's arguments.
    Argument {
        /// The argument holding the required address.
        argument: String,
        /// Whether the named argument authorizes a movement of value it does not
        /// itself own.
        ///
        /// `true` for `spender` in `transfer_from`, where the authority comes
        /// from a granted allowance rather than from the account's own balance.
        /// The distinction is the whole reason the method exists, so it is
        /// recorded rather than inferred from the argument's name.
        #[serde(default)]
        on_behalf_of: bool,
    },
    /// The principal is whoever invoked the call.
    Invoker,
    /// No principal is required, so the call must succeed whether or not a
    /// signature is supplied.
    None,
}

/// How the covered arguments must relate to the declared ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageMode {
    /// The covered set must equal the declared set.
    ///
    /// The strict reading a token interface requires: the contract must demand
    /// signatures over precisely the arguments that move value, neither fewer
    /// (which would leave a movement ungoverned) nor more (which would break
    /// callers that authorize exactly what is needed).
    Exact,
    /// The contract may cover more than the declared set.
    AtLeast,
    /// The contract may cover fewer, meaning it demands less authorization than
    /// the declaration lists.
    AtMost,
}

/// Which arguments the caller's authorization must cover.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationCoverage {
    /// How the covered set relates to `arguments`.
    pub mode: CoverageMode,
    /// The argument names that must be covered.
    pub arguments: Vec<String>,
}

/// What must happen on a given authorization path.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthorizationOutcome {
    /// The call must be accepted.
    Succeed,
    /// The call must be refused for the stated reason.
    Fail {
        /// Id of the `failures.yaml` entry this path must produce.
        ///
        /// Naming the failure rather than asserting a generic error is what stops
        /// a wrong-actor case from being satisfied by a contract that fails for
        /// an unrelated reason, such as an insufficient balance.
        failure: String,
    },
}
