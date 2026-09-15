//! Authorization.
//!
//! Authorization is a first-class conformance dimension, which means the runner
//! must be able to run a call **with** authorization and **without** it, and must
//! record what the contract actually demanded rather than what was offered.
//!
//! Those are two different things, and conflating them is how a test suite ends
//! up asserting that a call failed without ever establishing why.
//!
//! # What the host records, measured rather than assumed
//!
//! The demanded authorization is read from the host's own record of what it
//! *authenticated*, which the SDK exposes as `Env::auths`. That record is per
//! completed invocation, and this was established by measurement rather than by
//! reading the documentation, because the answer decides a whole dimension:
//!
//! * A call that **completes** reports every authorization it demanded, including
//!   the invocation tree each one covered. This holds under
//!   [`AuthorizationMode::Granted`], where `mock_all_auths` records the demand and
//!   approves it, so the contract's `require_auth` still runs.
//! * A call that is **refused** reports nothing at all — not "nothing was
//!   demanded", but no observation. The refusal unwinds the authorization manager
//!   before the snapshot is taken, and a refusal for a missing signature and a
//!   refusal for an insufficient balance arrive through the same channel.
//!
//! So a refused call leaves the demanded principals unobservable, and
//! [`AuthorizationRecord::recorded`] returning an empty list must never be read as
//! "this contract requires no authorization". The assertion layer distinguishes the
//! two cases; this module is where the distinction is produced.

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
            // still runs, and what it demanded is observable through `recorded`
            // for as long as the call completes.
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
    /// Read from the host's authorization snapshot, which holds the authorizations
    /// of the last **completed** invocation. An empty result therefore means "no
    /// authorization was authenticated", and it cannot distinguish a contract that
    /// demanded none from a call that was refused before its demand could be read.
    /// A caller that needs that distinction has to consult how the call ended; see
    /// this module's header for why.
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
