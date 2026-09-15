//! Events, end to end.
//!
//! An event requirement is the only thing that can see two of the defects in the fixture
//! set: a contract that moves value without announcing it, and one that announces a
//! movement twice. Neither is visible from the contract's own state, which is why the
//! dimension exists.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]

use estamora_core::ConformanceStatus;
use estamora_integration_tests as harness;

#[test]
fn a_contract_that_moves_value_without_emitting_an_event_is_caught() {
    // Every balance assertion holds for this fixture: it debits and credits exactly.
    // What fails is the requirement that the movement be announced, and the distinction
    // matters because an indexer reconstructing the ledger sees a change it cannot
    // attribute.
    let outcome = harness::run_fixture("omits-event");
    assert_eq!(
        harness::status(&outcome),
        ConformanceStatus::PartiallyConformant
    );
    harness::assert_failed(&outcome, "event/cardinality");
    harness::assert_failed(&outcome, "event/required");
    // The balance behaviour is correct and must be reported as such: a report that
    // failed everything would not tell a reader what the defect was.
    harness::assert_passed(&outcome, "state/sender-debited");
    harness::assert_passed(&outcome, "state/recipient-credited");
}

#[test]
fn emitting_the_event_twice_is_a_cardinality_failure() {
    // Both events are well formed and carry the right values, so a check that only
    // looked for *an* event would pass. The count is the requirement.
    let outcome = harness::run_fixture("double-emits");
    harness::assert_failed(&outcome, "event/cardinality");
    assert_eq!(
        harness::status(&outcome),
        ConformanceStatus::PartiallyConformant
    );
}

#[test]
fn an_event_that_states_the_wrong_amount_is_caught_by_its_data() {
    let outcome = harness::run_fixture("wrong-event-amount");
    harness::assert_failed(&outcome, "event/data");
    // The event's topics and its cardinality are right, so only the value check fails.
    harness::assert_passed(&outcome, "event/cardinality");
}

#[test]
fn the_conforming_fixture_emits_what_the_profile_requires() {
    let outcome = harness::run_fixture("none");
    for check in [
        "event/required",
        "event/cardinality",
        "event/topic",
        "event/data",
    ] {
        harness::assert_passed(&outcome, check);
    }
}

#[test]
fn a_refused_call_is_required_to_emit_nothing_and_does() {
    // The requirement is checked rather than assumed. It is also the requirement every
    // contract satisfies, because the host rolls a refused call's events back with the
    // invocation — which is stated in the profile rather than hidden, and pinned here so
    // that a platform change that stopped rolling back would show up as a failure.
    let outcome = harness::run_fixture("allows-overdraft");
    harness::assert_failed(&outcome, "event/forbidden");
}

#[test]
fn the_event_dimension_selects_by_tag() {
    let outcome = harness::run_fixture_with_tags("none", &["events"]);
    assert_eq!(
        harness::vector_ids(&outcome),
        vec!["transfer-moves-the-exact-amount"]
    );
}
