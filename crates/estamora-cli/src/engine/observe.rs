//! What the contract did, in the vocabulary the evaluator speaks.
//!
//! The evaluator never sees a ledger, and the execution layer never sees a
//! requirement. This module is the one place the two meet, and it is where two
//! distinctions have to survive the crossing intact.
//!
//! # A refusal is not a failure to observe
//!
//! A contract that rejects a call and an environment that could not produce an
//! observation are recorded as different things
//! ([`CallResult::Refused`](estamora_assertions::CallResult) and
//! [`CallResult::CouldNotRun`](estamora_assertions::CallResult)), because they lead to
//! opposite conclusions: one is evidence about the contract, and the other is
//! evidence about the runner. Collapsing them is how a network outage gets reported
//! as a contract that violated its profile.
//!
//! # An authorization is what the contract demanded, not what was offered
//!
//! Every recorded authorization is turned into an actor and the argument names their
//! signature covered, by comparing the authorized address against the call's own
//! argument values. That is what makes argument coverage checkable: a contract that
//! asks the right party to authorize but for the wrong arguments is a different
//! contract from one that asks for the right ones, and the difference is invisible
//! unless the comparison is made.
//!
//! # A value the runner cannot read is a failure to observe
//!
//! If a contract returns something this runner cannot carry, then no statement about
//! the return follows from the call. The call is recorded as
//! [`CallResult::CouldNotRun`](estamora_assertions::CallResult) rather than as a
//! return with a missing value, so that a requirement about the return is undecidable
//! rather than failed.

use estamora_assertions::dimensions::{InterfaceInspection, ObservedMethod, ObservedParameter};
use estamora_assertions::observation::{
    CallResult, ObservedAuthorization, ObservedCall, ObservedEvent,
};
use estamora_core::value::Value;
use estamora_soroban::{CallOutcome, ContractWorld, ExposedInterface, Invocation};
use soroban_sdk::Env;

/// The observation of one call.
///
/// `argument_names` are the profile's declared parameter names, in call order, and
/// `argument_values` are the values the call was made with in the same order. They are
/// supplied together so that the argument-coverage comparison cannot pair a name with
/// the wrong value.
#[must_use]
pub fn call(
    invocation: &Invocation,
    env: &Env,
    world: &ContractWorld<'_>,
    method: &str,
    argument_names: &[String],
    argument_values: &[Value],
) -> ObservedCall {
    let (outcome, returned) = match &invocation.outcome {
        CallOutcome::Returned => match invocation
            .returned
            .as_ref()
            .map(|value| world.from_host(value))
            .transpose()
        {
            // An absence is a void return rather than a value: a mutating method returns
            // nothing, and recording that as a value would make every requirement about
            // a return compare against something that was never returned.
            Ok(Some(Value::Absent)) => (CallResult::Returned, None),
            Ok(value) => (CallResult::Returned, value),
            // The call completed but what came back cannot be carried. Recorded as an
            // observation that did not happen rather than as a return with a missing
            // value, so a requirement about the return is undecidable rather than
            // failed.
            Err(problem) => (
                CallResult::CouldNotRun {
                    detail: problem.message().to_owned(),
                },
                None,
            ),
        },
        CallOutcome::Refused { code } => (CallResult::Refused { code: *code }, None),
        CallOutcome::Failed { detail, .. } => (
            CallResult::CouldNotRun {
                detail: detail.clone(),
            },
            None,
        ),
    };

    let events = invocation
        .events
        .iter()
        .map(|event| event_of(event, env, world))
        .collect::<Vec<ObservedEvent>>();

    let authorizations = invocation
        .authorizations
        .iter()
        .map(|record| {
            let actor = world
                .name_of(&record.actor)
                .map_or_else(|| record.actor.to_string().to_string(), str::to_owned);
            ObservedAuthorization {
                covers: covered_arguments(&actor, argument_names, argument_values),
                actor,
            }
        })
        .collect();

    ObservedCall {
        method: method.to_owned(),
        outcome,
        returned,
        events,
        authorizations,
        interface_verified: invocation.interface_verified,
    }
}

/// The argument names an authorized actor covered.
///
/// Derived by equality against the call's own values rather than by reading the
/// authorization's invocation tree, because the question is which *argument* the
/// principal corresponds to, and only the call knows what its arguments were called.
/// When no argument equals the actor — a contract that authorizes an account which
/// appears nowhere in the call — the result is empty rather than guessed at, and an
/// argument-coverage requirement then fails, which is the correct reading of a
/// contract whose authorization does not name what it acts on.
#[must_use]
fn covered_arguments(
    actor: &str,
    argument_names: &[String],
    argument_values: &[Value],
) -> Vec<String> {
    let mut covered = Vec::new();
    for (position, name) in argument_names.iter().enumerate() {
        if argument_values.get(position) == Some(&Value::Address(actor.to_owned())) {
            covered.push(name.clone());
        }
    }
    covered
}

/// One observed event.
///
/// A topic or payload this runner cannot carry is rendered as `Absent` rather than
/// aborting the capture. The alternative — failing the whole observation — would make
/// one unrepresentable event hide every other requirement in the vector, and an event
/// requirement is matched by name and by value, so an `Absent` topic simply does not
/// match what the profile declared.
fn event_of(
    event: &estamora_soroban::CapturedEvent,
    env: &Env,
    world: &ContractWorld<'_>,
) -> ObservedEvent {
    let name = event.name(env).map(|symbol| symbol.to_string());
    let topics = event
        .topics
        .iter()
        .map(|topic| world.from_host(&topic).unwrap_or(Value::Absent))
        .collect();
    let data = world.from_host(&event.data).unwrap_or(Value::Absent);
    ObservedEvent { name, topics, data }
}

/// The inspection the interface dimension compares against.
#[must_use]
pub fn inspection(interface: &ExposedInterface) -> InterfaceInspection {
    InterfaceInspection::inspected(
        interface
            .methods
            .iter()
            .map(|method| ObservedMethod {
                name: method.name.clone(),
                parameters: method
                    .parameters
                    .iter()
                    .map(|parameter| ObservedParameter {
                        name: parameter.name.clone(),
                        type_name: parameter.type_name.clone(),
                    })
                    .collect(),
                returns: method.returns.clone(),
                readonly: method.readonly,
            })
            .collect(),
        interface.source.clone(),
    )
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{covered_arguments, inspection};
    use estamora_core::value::Value;

    #[test]
    fn an_authorization_covers_the_arguments_it_names() {
        // Alice authorized, and Alice is the first argument, so the signature covers
        // `from` — which is precisely the claim `from.require_auth()` makes.
        let names = vec!["from".to_owned(), "to".to_owned(), "amount".to_owned()];
        let values = vec![
            Value::Address("alice".to_owned()),
            Value::Address("bob".to_owned()),
            Value::Integer(250),
        ];
        assert_eq!(covered_arguments("alice", &names, &values), vec!["from"]);
    }

    #[test]
    fn an_authorization_of_somebody_the_call_does_not_name_covers_nothing() {
        // Not an empty match: a contract that asks an unrelated account to authorize
        // has not covered what it acts on, and an argument-coverage requirement must
        // be able to see that.
        let names = vec!["from".to_owned(), "to".to_owned()];
        let values = vec![
            Value::Address("alice".to_owned()),
            Value::Address("bob".to_owned()),
        ];
        assert!(covered_arguments("mallory", &names, &values).is_empty());
    }

    #[test]
    fn an_actor_named_twice_in_the_arguments_covers_both_names() {
        // Reported as both rather than as the first, because the question the check
        // answers is which arguments the principal's signature reaches.
        let names = vec!["from".to_owned(), "to".to_owned()];
        let values = vec![
            Value::Address("alice".to_owned()),
            Value::Address("alice".to_owned()),
        ];
        assert_eq!(
            covered_arguments("alice", &names, &values),
            vec!["from", "to"]
        );
    }

    #[test]
    fn an_inspection_carries_the_source_so_a_report_can_say_how_it_was_established() {
        let interface = estamora_soroban::ExposedInterface::declared(
            &[estamora_soroban::DeclaredMethod {
                name: "balance",
                parameters: &["address"],
                returns: Some("i128"),
                readonly: true,
            }],
            "the fixture `none`",
        );
        let inspection = inspection(&interface);
        assert!(inspection.declares("balance"));
        assert_eq!(
            inspection.methods()[0].parameters[0].type_name,
            "address".to_owned()
        );
    }
}
