//! Authorization, end to end.
//!
//! The dimension this file measures is the one where a naive runner is most convincing
//! while being most wrong: a contract can require *a* signature without requiring the
//! right one, and every "an unauthorized call fails" test passes. So the tests here
//! check the plan, the refusal and the demanded principal separately, and they check
//! the case a naive suite cannot see at all — a contract that demanded nothing.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]

use estamora_core::ConformanceStatus;
use estamora_integration_tests as harness;

#[test]
fn a_contract_that_never_asks_for_authorization_is_caught() {
    // The fixture moves the value, credits the right account and emits the right event.
    // Everything about it is correct except that it never calls `require_auth`, and the
    // vector that constructs an unsigned call is the one that notices: the call
    // completed where the profile requires a refusal.
    let outcome = harness::run_fixture("skips-authorization");
    assert_eq!(
        harness::status(&outcome),
        ConformanceStatus::PartiallyConformant
    );
    harness::assert_failed(&outcome, "authorization/plan");
    harness::assert_failed(&outcome, "authorization/outcome");
    harness::assert_failed(&outcome, "failure/outcome");
}

#[test]
fn the_principal_the_contract_demanded_is_checked_where_it_can_be_observed() {
    // A contract that demanded no authorization at all completed the call, so the host's
    // record of what it authenticated is available and empty — and that is a checkable
    // fact rather than an absence of evidence. This is the case where the dimension
    // carries its full weight.
    let outcome = harness::run_fixture("skips-authorization");
    harness::assert_failed(&outcome, "authorization/principal");
    harness::assert_failed(&outcome, "authorization/coverage");
}

#[test]
fn the_conforming_fixture_satisfies_every_authorization_check() {
    let outcome = harness::run_fixture("none");
    harness::assert_passed(&outcome, "authorization/plan");
    harness::assert_passed(&outcome, "authorization/principal");
    harness::assert_passed(&outcome, "authorization/coverage");
    harness::assert_passed(&outcome, "authorization/outcome");
    assert!(
        harness::failures(&outcome).is_empty(),
        "the conforming contract must satisfy the authorization requirements too: {:#?}",
        harness::failures(&outcome)
    );
}

#[test]
fn a_refused_call_reports_the_demanded_principals_as_unobservable_rather_than_absent() {
    // The distinction the whole dimension turns on. A refused call leaves no record of
    // what the contract demanded — the refusal unwinds it — so the runner cannot say
    // whether the contract checked the right principal. Reporting that as a *violated*
    // requirement would blame the contract for the runner's blind spot; reporting it as
    // satisfied would claim evidence that does not exist. It is reported as a finding.
    let outcome = harness::run_fixture("none");
    let unsigned = outcome
        .outcomes
        .iter()
        .find(|vector| vector.vector_id == "transfer-without-authorization-fails")
        .expect("the corpus declares the unsigned-call vector");

    assert!(
        unsigned
            .diagnostics
            .iter()
            .any(|finding| finding.code.starts_with("authorization-unobservable")),
        "the reason the check could not be made must be recorded: {:#?}",
        unsigned.diagnostics
    );
    // The evaluator's own identifiers are asserted on rather than the report's, because
    // these are the outcomes before the report normalises them to the schema's identifier
    // grammar. Both name the same checks; the structured form is the one that can be
    // matched without reconstructing a hash.
    assert!(
        !unsigned.assertions.iter().any(|assertion| assertion
            .id
            .starts_with("authorization/principal")
            || assertion.id.starts_with("authorization/coverage")),
        "a check that could not be made must not appear as one that was: {:#?}",
        unsigned.assertions
    );
    // And the refusal itself is still checked, which is what discriminates the contract.
    assert!(
        unsigned
            .assertions
            .iter()
            .any(|assertion| assertion.id.starts_with("authorization/outcome")),
        "the refusal must still be checked: {:#?}",
        unsigned.assertions
    );
}

#[test]
fn the_unsigned_vector_selects_by_tag_without_the_rest_of_the_corpus() {
    // A caller narrowing a run to one requirement must still get that requirement's
    // checks, and the tag must select by what the vector is about rather than by its
    // method alone.
    let outcome = harness::run_fixture_with_tags("none", &["authorization"]);
    assert_eq!(
        harness::vector_ids(&outcome),
        vec!["transfer-without-authorization-fails"]
    );
    harness::assert_passed(&outcome, "authorization/outcome");
}
