//! Execution tests.
//!
//! Every test here pins a behaviour the assertion layer depends on, including the
//! ones that decide whether a refusal counts as a failure. If the host's
//! semantics change, these fail, and they are supposed to: a change in what an
//! abort means would silently change what every profile concludes.
//!
//! Two of these tests record behaviour that was *established by experiment*
//! rather than assumed, and both are load-bearing:
//!
//! - a method that does not exist aborts exactly like a deliberate `panic!()`, so
//!   a failed call is not evidence of a refusal unless the interface was checked
//!   first;
//! - a refused call's events and mutations are not observable, so a requirement
//!   that a refusal must be free of effects is enforceable.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests may panic; a failing test is the signal"
)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
use soroban_sdk::{Address, Env, IntoVal, TryFromVal as _, Val, Vec, symbol_short};

use crate::auth::{self, AuthorizationMode};
use crate::fixtures::ConformingToken;
use crate::host::{LedgerPoint, LocalHost};
use crate::invoker::{CallOutcome, invoke};

/// A host with a registered conforming token and three accounts.
fn scenario() -> (Env, Address, Address, Address, Address) {
    let host = LocalHost::new(LedgerPoint::new(1_000, 1_768_478_400));
    let env = host.env().clone();
    let contract = host.register(ConformingToken);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let mallory = Address::generate(&env);
    (env, contract, alice, bob, mallory)
}

fn mint(env: &Env, contract: &Address, who: &Address, amount: i128) {
    let _: () = env.invoke_contract(
        contract,
        &symbol_short!("mint"),
        (who.clone(), amount).into_val(env),
    );
}

fn balance(env: &Env, contract: &Address, who: &Address) -> i128 {
    env.invoke_contract(
        contract,
        &symbol_short!("balance"),
        (who.clone(),).into_val(env),
    )
}

#[test]
fn the_ledger_point_is_the_one_the_fixture_declared() {
    let host = LocalHost::new(LedgerPoint::new(4_242, 1_768_478_400));
    assert_eq!(host.env().ledger().sequence(), 4_242);
    assert_eq!(host.env().ledger().timestamp(), 1_768_478_400);
}

#[test]
fn a_read_only_call_returns_the_contract_s_value() {
    let (env, contract, _, _, _) = scenario();
    let observed = invoke(&env, &contract, "decimals", Vec::new(&env), true).unwrap();

    assert_eq!(observed.outcome, CallOutcome::Returned);
    assert!(observed.accepted());
    let returned = observed.returned.expect("a value was returned");
    assert_eq!(u32::try_from_val(&env, &returned).unwrap(), 7);
}

#[test]
fn an_authorized_transfer_moves_exactly_the_requested_amount() {
    let (env, contract, alice, bob, _) = scenario();
    mint(&env, &contract, &alice, 1_000);
    AuthorizationMode::Granted.apply(&env);

    let observed = invoke(
        &env,
        &contract,
        "transfer",
        (alice.clone(), bob.clone(), 250_i128).into_val(&env),
        true,
    )
    .unwrap();

    assert!(observed.accepted());
    assert_eq!(balance(&env, &contract, &alice), 750);
    assert_eq!(balance(&env, &contract, &bob), 250);
}

#[test]
fn a_successful_transfer_emits_one_event_named_in_its_first_topic() {
    let (env, contract, alice, bob, _) = scenario();
    mint(&env, &contract, &alice, 1_000);
    AuthorizationMode::Granted.apply(&env);

    let observed = invoke(
        &env,
        &contract,
        "transfer",
        (alice.clone(), bob.clone(), 250_i128).into_val(&env),
        true,
    )
    .unwrap();

    assert_eq!(observed.events.len(), 1);
    assert_eq!(observed.event_count(&env, "transfer"), 1);
    let event = observed.events.first().expect("one event");
    assert_eq!(event.contract, contract);
    assert_eq!(event.name(&env), Some(symbol_short!("transfer")));
    // The topic list is the name plus one topic per declared `#[topic]` field.
    assert_eq!(event.topic_count(), 3);
}

#[test]
fn a_name_based_requirement_counts_only_events_with_that_name() {
    let (env, contract, alice, bob, _) = scenario();
    mint(&env, &contract, &alice, 1_000);
    AuthorizationMode::Granted.apply(&env);

    let observed = invoke(
        &env,
        &contract,
        "transfer",
        (alice.clone(), bob.clone(), 250_i128).into_val(&env),
        true,
    )
    .unwrap();

    // A requirement naming `transfer` must not be satisfied by *any* event, and
    // a requirement naming an absent event must not be satisfied by a present
    // one. Both directions are checked because a matcher that ignores the name
    // would pass the first and a matcher that ignores the event would pass the
    // second.
    assert_eq!(observed.event_count(&env, "transfer"), 1);
    assert_eq!(observed.event_count(&env, "sneaky"), 0);
}

#[test]
fn a_transfer_without_authorization_is_refused_and_moves_nothing() {
    let (env, contract, alice, bob, _) = scenario();
    mint(&env, &contract, &alice, 1_000);
    // No authorization is supplied. This is the observation a "must require
    // authorization" requirement depends on.
    AuthorizationMode::Denied.apply(&env);

    let observed = invoke(
        &env,
        &contract,
        "transfer",
        (alice.clone(), bob.clone(), 250_i128).into_val(&env),
        true,
    )
    .unwrap();

    assert!(observed.refused(), "outcome was {:?}", observed.outcome);
    assert_eq!(balance(&env, &contract, &alice), 1_000);
    assert_eq!(balance(&env, &contract, &bob), 0);
    assert!(
        observed.events.is_empty(),
        "a refused call must emit nothing"
    );
}

#[test]
fn a_transfer_authorized_by_the_wrong_actor_is_refused() {
    let (env, contract, alice, bob, mallory) = scenario();
    mint(&env, &contract, &alice, 1_000);

    // Mallory authorizes the call. The contract must ask Alice, so the call must
    // be refused even though *some* valid authorization was supplied.
    let args: Vec<Val> = (alice.clone(), bob.clone(), 250_i128).into_val(&env);
    let invocation = MockAuthInvoke {
        contract: &contract,
        fn_name: "transfer",
        args,
        sub_invokes: &[],
    };
    let auths = [MockAuth {
        address: &mallory,
        invoke: &invocation,
    }];
    auth::apply_authorizations(&env, &auths);

    let observed = invoke(
        &env,
        &contract,
        "transfer",
        (alice.clone(), bob.clone(), 250_i128).into_val(&env),
        true,
    )
    .unwrap();

    assert!(observed.refused(), "outcome was {:?}", observed.outcome);
    assert_eq!(balance(&env, &contract, &alice), 1_000);
}

#[test]
fn a_contract_that_skips_its_authorization_check_is_caught() {
    let (env, contract, alice, bob, _) = scenario();
    mint(&env, &contract, &alice, 1_000);
    AuthorizationMode::Denied.apply(&env);

    // The defect the authorization dimension exists to catch: the call succeeds
    // with no authorization supplied at all.
    let observed = invoke(
        &env,
        &contract,
        "transfer_unchecked",
        (alice.clone(), bob.clone(), 250_i128).into_val(&env),
        true,
    )
    .unwrap();

    assert!(observed.accepted());
    assert_eq!(balance(&env, &contract, &bob), 250);
}

#[test]
fn a_transfer_beyond_the_balance_is_refused_and_moves_nothing() {
    let (env, contract, alice, bob, _) = scenario();
    mint(&env, &contract, &alice, 100);
    AuthorizationMode::Granted.apply(&env);

    let observed = invoke(
        &env,
        &contract,
        "transfer",
        (alice.clone(), bob.clone(), 250_i128).into_val(&env),
        true,
    )
    .unwrap();

    assert!(observed.refused(), "outcome was {:?}", observed.outcome);
    assert_eq!(balance(&env, &contract, &alice), 100);
    assert_eq!(balance(&env, &contract, &bob), 0);
}

#[test]
fn a_bare_panic_is_observed_as_a_refusal_without_an_error_code() {
    let (env, contract, _, _, _) = scenario();
    let observed = invoke(&env, &contract, "always_refuses", Vec::new(&env), true).unwrap();

    // A bare `panic!()` carries no contract error code. It is still a refusal:
    // it is the dominant idiom for rejecting a call, and treating it as an
    // environment failure would misclassify most conforming contracts.
    assert_eq!(observed.outcome, CallOutcome::Refused { code: None });
}

#[test]
fn a_method_that_does_not_exist_aborts_exactly_like_a_deliberate_refusal() {
    let (env, contract, _, _, _) = scenario();

    // Established by experiment, and the reason the interface layer exists:
    // Soroban reports a missing method and a deliberate `panic!()` through the
    // same channel. The two are indistinguishable from the call alone, so
    // `interface_verified` is the only thing that licenses reading this as a
    // refusal, and the assertion layer must refuse to count it as satisfying a
    // "must fail" requirement when it is false.
    let observed = invoke(&env, &contract, "no_such_method", Vec::new(&env), false).unwrap();

    assert_eq!(observed.outcome, CallOutcome::Refused { code: None });
    assert!(
        !observed.abort_is_unambiguous(),
        "a refusal observed without interface verification must not be read as a contract decision"
    );
}

#[test]
fn events_from_a_refused_call_are_not_observable() {
    let (env, contract, alice, _, _) = scenario();
    let observed = invoke(
        &env,
        &contract,
        "emits_then_refuses",
        (alice.clone(),).into_val(&env),
        true,
    )
    .unwrap();

    assert!(observed.refused());
    // Soroban rolls a refused call back, events included. If this ever changed,
    // the requirement "a refusal must emit nothing" would become unenforceable,
    // which is why the behaviour is pinned rather than assumed.
    assert!(
        observed.events.is_empty(),
        "a refusal's events were captured: {:?}",
        observed.events.len()
    );
}

#[test]
fn state_mutated_before_a_refusal_is_not_observable() {
    let (env, contract, alice, _, _) = scenario();
    let observed = invoke(
        &env,
        &contract,
        "mutates_then_refuses",
        (alice.clone(), 500_i128, true).into_val(&env),
        true,
    )
    .unwrap();

    assert!(observed.refused());
    assert_eq!(
        balance(&env, &contract, &alice),
        0,
        "a refused call left a mutation behind"
    );
}

#[test]
fn recorded_authorizations_name_the_actor_the_contract_asked() {
    let (env, contract, alice, bob, _) = scenario();
    mint(&env, &contract, &alice, 1_000);
    AuthorizationMode::Granted.apply(&env);

    let observed = invoke(
        &env,
        &contract,
        "transfer",
        (alice.clone(), bob.clone(), 250_i128).into_val(&env),
        true,
    )
    .unwrap();

    assert_eq!(observed.authorizations.len(), 1);
    assert_eq!(observed.authorizations[0].actor, alice);
}
