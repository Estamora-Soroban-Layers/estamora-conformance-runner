//! Failure behaviour, end to end.
//!
//! A profile that described only successful behaviour would accept a contract that
//! silently succeeds where the standard requires a refusal — the most dangerous kind of
//! non-conformance, because every happy-path test misses it. These tests check the two
//! halves separately: that a refusal the profile requires happened, and that the
//! contract's own refusal was classified as the failure the vector named.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]

use estamora_core::{ConformanceStatus, ErrorClass};
use estamora_integration_tests as harness;

#[test]
fn a_contract_that_settles_a_transfer_it_must_refuse_is_caught() {
    let outcome = harness::run_fixture("allows-overdraft");
    assert_eq!(
        harness::status(&outcome),
        ConformanceStatus::PartiallyConformant
    );
    harness::assert_failed(
        &outcome,
        "behavior/transfer-beyond-the-balance-fails/outcome",
    );
    harness::assert_failed(&outcome, "failure/outcome");
}

#[test]
fn the_conforming_fixture_refuses_what_the_profile_requires_it_to_refuse() {
    let outcome = harness::run_fixture("none");
    for check in [
        "failure/outcome",
        "behavior/transfer-beyond-the-balance-fails/failure",
        "behavior/transfer-beyond-the-balance-fails/outcome",
    ] {
        harness::assert_passed(&outcome, check);
    }
}

#[test]
fn a_refusal_is_not_accepted_as_a_failure_of_the_wrong_kind() {
    // The vector constructs a missing-authorization scenario. The rule for an
    // over-balance refusal must not claim it, and vice versa: a requirement applied to a
    // scenario it does not describe reports a pass for something that was never tested.
    let outcome = harness::run_fixture("none");
    let unsigned = outcome
        .outcomes
        .iter()
        .find(|vector| vector.vector_id == "transfer-without-authorization-fails")
        .expect("the corpus declares the unsigned vector");
    assert!(
        unsigned
            .diagnostics
            .iter()
            .any(|finding| finding.message.contains("this rule does not describe")),
        "the over-balance rule must record that the vector's failure is not the one it \
         describes: {:#?}",
        unsigned.diagnostics
    );
}

#[test]
fn an_unusable_profile_is_a_specification_error_and_not_a_contract_one() {
    // The most damaging mistake the runner could make is to blame a contract for a
    // requirement nobody could read. A profile that cannot be loaded stops the run with
    // exit `3` and never reaches a verdict.
    let problem = harness::run_bundle(
        &harness::repo_root().join("fixtures/profiles/conformance-token"),
        "none",
    )
    .expect_err("the directory holds no profile bundle");
    assert_eq!(problem.class(), ErrorClass::ProfileError);
    assert_eq!(
        estamora_cli::exit_code(problem.class()),
        estamora_core::ExitCode::SpecificationInvalid
    );
    assert_eq!(problem.blame(), estamora_core::Blame::Specification);
}

#[test]
fn a_failing_run_reports_the_contract_and_exits_one() {
    // The other half of the classification: a contract that violates a requirement is a
    // report rather than an error, and its exit code is the one CI gates on.
    let outcome = harness::run_fixture("allows-overdraft");
    assert!(harness::status(&outcome).describes_contract());
    assert_eq!(
        harness::status(&outcome).exit_code(),
        estamora_core::ExitCode::NonConformant
    );
    assert!(!harness::failures(&outcome).is_empty());
}
