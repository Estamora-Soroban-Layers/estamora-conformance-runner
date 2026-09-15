//! Authorization.
//!
//! Authorization is a first-class conformance dimension, which means the runner
//! must be able to run a call **with** authorization and **without** it, and must
//! record what the contract actually demanded rather than what was offered.
//!
//! Those are two different things, and conflating them is how a test suite ends
//! up asserting that a call failed without ever establishing why.

use soroban_sdk::testutils::{AuthorizedInvocation, MockAuth};
use soroban_sdk::{Address, Env};

/// How a call's authorization is supplied.
///
/// The mode is a property of the *scenario*, not of the contract: a profile's
/// positive vectors run [`AuthorizationMode::Granted`] because the contract is
/// entitled to assume its caller is authorized, and a profile's negative vectors
/// run [`AuthorizationMode::Denied`] because that is the only way to observe
/// whether the check exists at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationMode {
    /// No authorization is supplied. A call that requires any is refused by the
    /// host before it reaches the contract, which is exactly the observation a
    /// "must require authorization" vector needs.
    Denied,
    /// Every authorization the call demands is granted. Used for vectors that
    /// test behaviour other than authorization, so that an unrelated authorization
    /// failure does not masquerade as a behavioural defect.
    Granted,
}

impl AuthorizationMode {
    /// Applies the mode to `env`.
    pub fn apply(self, env: &Env) {
        match self {
            // `mock_all_auths` records the auths the call demanded and approves
            // them. It is not "skip the check": the contract's `require_auth`
            // still runs, and what it demanded is still observable through
            // `recorded`.
            Self::Granted => env.mock_all_auths(),
            Self::Denied => {},
        }
    }
}

/// One authorization the contract required during a call.
///
/// Recorded even when the mode granted it, because a vector may need to assert
/// *which* actor was asked to authorize and for which arguments. That is the
/// difference between a contract that checks the right party's signature and one
/// that checks whoever happens to be first.
#[derive(Debug, Clone)]
pub struct AuthorizationRecord {
    /// The account or contract that was asked to authorize.
    pub actor: Address,
    /// The invocation tree the actor authorized.
    pub invocation: AuthorizedInvocation,
}

impl AuthorizationRecord {
    /// Reads every authorization the last call required on `env`.
    ///
    /// The SDK exposes this from the host's authorization snapshot, so it is
    /// available whether the authorization was supplied or withheld, and it is
    /// cleared between calls by the host.
    #[must_use]
    pub fn recorded(env: &Env) -> Vec<Self> {
        env.auths()
            .into_iter()
            .map(|(actor, invocation)| Self { actor, invocation })
            .collect()
    }
}

/// Applies a specific set of authorizations to `env`.
///
/// This is what a wrong-actor scenario needs and `AuthorizationMode::Granted`
/// cannot express: the call must be authorized by *somebody*, just not by the
/// party the contract should have required. Authorizing everyone would let a
/// contract that checks the wrong account pass, and authorizing nobody would
/// fail the call before the wrong account could be observed.
///
/// The authorizations are borrowed from the caller rather than constructed here,
/// so that a scenario owns the values it depends on and no ownership is
/// smuggled through a leaked allocation.
pub fn apply_authorizations(env: &Env, authorizations: &[MockAuth<'_>]) {
    env.mock_auths(authorizations);
}
