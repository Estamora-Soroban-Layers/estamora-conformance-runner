//! The authorization dimension.
//!
//! Authorization is a conformance dimension in its own right, and it is the one
//! where a naive runner is most convincing while being most wrong. Checking that a
//! call without a signature is refused establishes almost nothing: a contract that
//! verifies *that* some signature is present, without verifying *whose*, passes
//! that test and is fully exploitable. So this dimension keeps three cases apart —
//! authorized success, unauthorized failure and wrong-actor failure — and it checks
//! two things the naive test cannot see: **who** the contract demanded, and **which
//! arguments** their authorization covered.
//!
//! # Coverage is the requirement most profiles forget to state
//!
//! SEP-41-style interfaces require a `transfer` to be authorized by the debited
//! account *over the `from` argument specifically*. A contract that requires a
//! signature but never binds it to the account it debits will move value out of an
//! account that never authorized the movement while passing every
//! "unauthorized call fails" test. The declared coverage in `authorization.yaml`
//! is what makes that a checkable requirement instead of an aspiration.
//!
//! # A refusal is only a refusal if the method exists
//!
//! Every outcome check here consults [`ObservedCall::refusal_is_unambiguous`]. A
//! contract that does not implement `transfer` at all refuses every call to it, and
//! would otherwise satisfy "unauthorized transfers must fail" perfectly.

use estamora_core::Result;
use estamora_profile::ProfileBundle;
use estamora_profile::authorization::{
    AuthorizationActor, AuthorizationOutcome, AuthorizationRule, CoverageMode,
};
use estamora_vectors::{
    AssertionCategory, AuthorizationExpectation, AuthorizationPlan, VectorDocument,
};

use crate::eval::{Environment, evaluate_value};
use crate::observation::ObservedCall;
use crate::outcome::AssertionOutcome;
use crate::value::Value;

/// Which authorization path a vector exercises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationPath {
    /// A signature was supplied by the required principal.
    Authorized,
    /// No signature was supplied at all.
    Unauthorized,
    /// A signature was supplied, by the wrong principal.
    WrongActor,
}

impl AuthorizationPath {
    /// The path a vector's plan constructs.
    ///
    /// The plan is the only thing that knows: a contract cannot be asked what it
    /// would have done under a different signature, so the path is a property of
    /// the scenario the vector built rather than of the observation.
    #[must_use]
    pub fn of(plan: &AuthorizationPlan) -> Self {
        if !plan.substituted_for.is_empty() {
            return Self::WrongActor;
        }
        if plan.actors.is_empty() {
            return Self::Unauthorized;
        }
        Self::Authorized
    }

    /// The stable machine-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Authorized => "authorized",
            Self::Unauthorized => "unauthorized",
            Self::WrongActor => "wrong_actor",
        }
    }
}

/// Evaluates the authorization dimension for one call.
///
/// # Errors
///
/// Returns an error when a value the requirement names could not be computed — a
/// declared input that cannot be resolved, or a read that failed. Neither is a
/// statement about the contract.
pub fn evaluate(
    profile: &ProfileBundle,
    vector: &VectorDocument,
    environment: &Environment<'_>,
    call: &ObservedCall,
) -> Result<Vec<AssertionOutcome>> {
    let path = AuthorizationPath::of(&vector.authorization);
    let rules: Vec<&AuthorizationRule> = profile
        .documents()
        .authorization
        .authorization_rules
        .iter()
        .filter(|rule| {
            rule.methods
                .iter()
                .any(|method| method == &vector.method || method == "*")
        })
        .collect();

    let mut outcomes = vec![plan_outcome(vector, call, path)];

    for rule in rules {
        outcomes.push(principal_outcome(rule, environment, call)?);
        outcomes.push(coverage_outcome(rule, call));
        if let Some(outcome) = path_outcome(rule, path, call) {
            outcomes.push(outcome);
        }
    }

    Ok(outcomes)
}

/// Whether the call's acceptance matches what the plan said the signing should
/// achieve.
fn plan_outcome(
    vector: &VectorDocument,
    call: &ObservedCall,
    path: AuthorizationPath,
) -> AssertionOutcome {
    let id = format!("authorization/plan/{}", vector.id);
    let expected = match vector.authorization.expected {
        AuthorizationExpectation::Accepted => "the call is accepted",
        AuthorizationExpectation::Rejected => "the call is refused",
        AuthorizationExpectation::NotRequired => "the call is accepted with no signature",
    };
    let observed = describe(call);
    let held = match vector.authorization.expected {
        AuthorizationExpectation::Accepted | AuthorizationExpectation::NotRequired => {
            call.accepted()
        },
        // A required refusal that arrived through an abort is only the contract's
        // decision when the method was known to exist. Without that, the abort
        // could be the missing method, and the plan would be satisfied by a
        // contract that does not implement the operation at all.
        AuthorizationExpectation::Rejected => call.refusal_is_unambiguous(),
    };
    AssertionOutcome::from_boolean(
        id,
        AssertionCategory::Authorization,
        expected,
        observed,
        held,
    )
    .with_detail(format!("authorization path: {}", path.as_str()))
}

/// Whether the principal the profile requires is the one the contract demanded.
fn principal_outcome(
    rule: &AuthorizationRule,
    environment: &Environment<'_>,
    call: &ObservedCall,
) -> Result<AssertionOutcome> {
    let id = format!("authorization/principal/{}", rule.id);
    let observed = describe_actors(call);

    let expected = match &rule.actor {
        AuthorizationActor::None => {
            return Ok(AssertionOutcome::from_boolean(
                id,
                AssertionCategory::Authorization,
                "no principal is required to authorize",
                observed,
                call.authorizations.is_empty(),
            )
            .with_detail(format!(
                "the profile declares that {} requires no authorization",
                rule.methods.join(", ")
            )));
        },
        AuthorizationActor::Invoker => "the invoking principal".to_owned(),
        AuthorizationActor::Argument {
            argument,
            on_behalf_of,
        } => {
            let required = principal_from_argument(argument, environment)?;
            let qualifier = if *on_behalf_of {
                " (authorizing a movement of value it does not itself hold)"
            } else {
                ""
            };
            match required {
                Some(name) => format!("{name}{qualifier}"),
                None => {
                    // The argument could not be read as an address. That is a
                    // vector or method defect rather than a contract finding, and
                    // saying so is better than reporting a contract failure
                    // derived from an unresolvable value.
                    return Ok(AssertionOutcome::failed(
                        id,
                        AssertionCategory::Authorization,
                        format!("the address held by `{argument}`{qualifier}"),
                        observed,
                    )
                    .with_detail(format!(
                        "argument `{argument}` did not resolve to an actor address, so the \
                         required principal could not be determined; the requirement was not \
                         applied to the contract"
                    )));
                },
            }
        },
    };

    let required_name = match &rule.actor {
        AuthorizationActor::Argument { argument, .. } => {
            principal_from_argument(argument, environment)?
        },
        _ => None,
    };
    let held = match &rule.actor {
        AuthorizationActor::Invoker => {
            // The invoking principal is whoever the vector authorized, and the
            // observation records it by name.
            !call.authorizing_actors().is_empty()
        },
        AuthorizationActor::Argument { .. } => match required_name {
            Some(name) => call.authorizing_actors().iter().any(|actor| *actor == name),
            None => false,
        },
        AuthorizationActor::None => call.authorizations.is_empty(),
    };

    Ok(AssertionOutcome::from_boolean(
        id,
        AssertionCategory::Authorization,
        expected,
        observed,
        held,
    ))
}

/// Whether the arguments the contract demanded authorization over match the
/// profile's declaration.
fn coverage_outcome(rule: &AuthorizationRule, call: &ObservedCall) -> AssertionOutcome {
    let id = format!("authorization/coverage/{}", rule.id);
    let declared: Vec<String> = {
        let mut names = rule.coverage.arguments.clone();
        names.sort();
        names
    };
    let covered = call.covered_arguments();

    let held = match rule.coverage.mode {
        CoverageMode::Exact => covered == declared,
        CoverageMode::AtLeast => declared.iter().all(|name| covered.contains(name)),
        CoverageMode::AtMost => covered.iter().all(|name| declared.contains(name)),
    };

    AssertionOutcome::from_boolean(
        id,
        AssertionCategory::Authorization,
        format!("{}({})", mode_name(rule.coverage.mode), declared.join(", ")),
        if covered.is_empty() {
            "<no arguments>".to_owned()
        } else {
            covered.join(", ")
        },
        held,
    )
    .with_detail(format!(
        "the coverage the contract demanded was derived by matching each authorized address \
         against the call's own argument values, so it is what the contract required rather \
         than what the vector offered; mode: {}",
        mode_name(rule.coverage.mode)
    ))
}

/// Whether the path the vector constructed produced what the rule requires for it.
fn path_outcome(
    rule: &AuthorizationRule,
    path: AuthorizationPath,
    call: &ObservedCall,
) -> Option<AssertionOutcome> {
    let declared = match path {
        AuthorizationPath::Unauthorized => &rule.unauthorized,
        AuthorizationPath::WrongActor => &rule.wrong_actor,
        // The authorized path's requirement is the principal check, not an
        // outcome check: an authorized call may still be refused for a reason
        // that has nothing to do with authorization, such as an insufficient
        // balance, and requiring acceptance here would make that a conformance
        // failure.
        AuthorizationPath::Authorized => return None,
    };

    let id = format!("authorization/outcome/{}/{}", rule.id, path.as_str());
    let held = match declared {
        AuthorizationOutcome::Succeed => call.accepted(),
        // A refusal that arrived through an abort is only the contract's own
        // decision when the method was known to exist: without that, a missing
        // method refuses every call to it and would satisfy "unauthorized calls
        // must fail" perfectly.
        //
        // The *reason* for the refusal is established by the failure dimension,
        // which knows the failure definition's category and signal requirements.
        // What is checkable here is that a genuine refusal happened on the path
        // the vector constructed.
        AuthorizationOutcome::Fail { .. } => call.refusal_is_unambiguous(),
    };

    Some(
        AssertionOutcome::from_boolean(
            id,
            AssertionCategory::Authorization,
            describe_outcome(declared),
            describe(call),
            held,
        )
        .with_detail(match (path, declared) {
            (_, AuthorizationOutcome::Fail { failure }) => format!(
                "the profile requires this refusal to be classified as `{failure}`; the \
                 classification itself is reported by the failure dimension"
            ),
            _ => format!("path: {}", path.as_str()),
        }),
    )
}

/// Resolves the actor an `argument` authorization names.
///
/// # Errors
///
/// Returns an error when the value could not be computed.
fn principal_from_argument(
    argument: &str,
    environment: &Environment<'_>,
) -> Result<Option<String>> {
    let Some(expr) = environment.inputs.get(argument) else {
        return Ok(None);
    };
    let value = evaluate_value(environment, expr)?;
    Ok(actor_name(value))
}

/// The fixture actor name a value denotes, where it denotes one.
fn actor_name(value: Value) -> Option<String> {
    match value {
        Value::Address(name) | Value::Text(name) => Some(name),
        _ => None,
    }
}

/// A rendering of every principal the contract demanded authorization from.
fn describe_actors(call: &ObservedCall) -> String {
    if call.authorizations.is_empty() {
        return "<no authorization required>".to_owned();
    }
    call.authorizations
        .iter()
        .map(|authorization| {
            if authorization.covers.is_empty() {
                authorization.actor.clone()
            } else {
                format!(
                    "{} over {}",
                    authorization.actor,
                    authorization.covers.join(", ")
                )
            }
        })
        .collect::<Vec<String>>()
        .join("; ")
}

/// A rendering of what was observed about the call.
fn describe(call: &ObservedCall) -> String {
    match &call.outcome {
        crate::observation::CallResult::Returned => "accepted".to_owned(),
        crate::observation::CallResult::Refused { code } => match code {
            Some(code) => format!("refused with code {code}"),
            None => "refused".to_owned(),
        },
        crate::observation::CallResult::CouldNotRun { detail } => {
            format!("could not run: {detail}")
        },
    }
}

/// A rendering of what a rule requires on a path.
fn describe_outcome(outcome: &AuthorizationOutcome) -> String {
    match outcome {
        AuthorizationOutcome::Succeed => "accepted".to_owned(),
        AuthorizationOutcome::Fail { failure } => format!("refused as `{failure}`"),
    }
}

/// The document spelling of a coverage mode.
fn mode_name(mode: CoverageMode) -> &'static str {
    match mode {
        CoverageMode::Exact => "exactly",
        CoverageMode::AtLeast => "at least",
        CoverageMode::AtMost => "at most",
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{AuthorizationPath, actor_name};
    use crate::value::Value;
    use estamora_vectors::AuthorizationPlan;

    #[test]
    fn a_plan_with_no_actors_is_the_unauthorized_path() {
        let plan = AuthorizationPlan {
            actors: Vec::new(),
            expected: estamora_vectors::AuthorizationExpectation::Rejected,
            substituted_for: Vec::new(),
        };
        assert_eq!(
            AuthorizationPath::of(&plan),
            AuthorizationPath::Unauthorized
        );
    }

    #[test]
    fn a_substitution_wins_over_absence() {
        let plan = AuthorizationPlan {
            actors: vec!["mallory".to_owned()],
            expected: estamora_vectors::AuthorizationExpectation::Rejected,
            substituted_for: vec!["alice".to_owned()],
        };
        assert_eq!(AuthorizationPath::of(&plan), AuthorizationPath::WrongActor);
    }

    #[test]
    fn only_an_address_or_text_names_an_actor() {
        assert_eq!(
            actor_name(Value::Address("alice".to_owned())).as_deref(),
            Some("alice")
        );
        assert_eq!(actor_name(Value::Integer(1)), None);
    }
}
