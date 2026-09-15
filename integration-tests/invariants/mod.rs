//! Invariants, end to end.
//!
//! An invariant is the requirement that survives an operation rather than describing one,
//! so these tests are about the properties a contract cannot satisfy by being correct
//! about a single call. Conservation is evaluated over the whole balance set, which is
//! what makes it able to see a movement into an account the operation never named.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]

use estamora_core::ConformanceStatus;
use estamora_integration_tests as harness;

#[test]
fn a_fee_taken_in_silence_breaks_conservation() {
    // The fixture debits the sender by the amount and credits the recipient by one less,
    // so a value leaves the two named accounts without any event having said so. Each
    // pairwise delta is plausible on its own; the sum over the set is what notices.
    let outcome = harness::run_fixture("wrong-credit-amount");
    harness::assert_failed(&outcome, "invariant/conservation");
    assert_eq!(
        harness::status(&outcome),
        ConformanceStatus::PartiallyConformant
    );
}

#[test]
fn an_overdraft_breaks_the_non_negative_bound() {
    // Scoped to every method and both outcomes, so it is evaluated on the vector that
    // must fail as well as on the one that must succeed. A contract whose arithmetic
    // wraps has produced a quantity the interface cannot represent, and that is a
    // stronger statement than "the transfer failed".
    let outcome = harness::run_fixture("allows-overdraft");
    harness::assert_failed(&outcome, "invariant/bounds");
}

#[test]
fn the_conforming_fixture_survives_every_invariant() {
    let outcome = harness::run_fixture("none");
    for check in [
        "invariant/conservation/balances-are-conserved-by-transfer",
        "invariant/bounds/balances-are-non-negative",
        "invariant/state_unchanged/failed-transfer-cannot-mutate",
    ] {
        harness::assert_passed(&outcome, check);
    }
    let failed = harness::failures(&outcome);
    assert!(
        failed.iter().all(|check| !check.contains("invariant/")),
        "no invariant should have been violated: {failed:#?}"
    );
}

#[test]
fn an_invariant_outside_its_scope_is_recorded_as_inapplicable_rather_than_satisfied() {
    // The conservation invariant is scoped to the success outcome of `transfer`. On the
    // vector that must be refused it must not be reported as a pass: counting it would
    // claim coverage the scenario never exercised, which is the same defect as skipping
    // it silently.
    let outcome = harness::run_fixture("none");
    let refused = outcome
        .outcomes
        .iter()
        .find(|vector| vector.vector_id == "transfer-beyond-the-balance-fails")
        .expect("the corpus declares the over-balance vector");
    assert!(
        !refused
            .assertions
            .iter()
            .any(|assertion| assertion.id.starts_with("invariant/conservation")),
        "a conservation check must not be reported for a refused call: {:#?}",
        refused.assertions
    );
    // The bound is scoped to both outcomes, so it *is* reported there.
    assert!(
        refused
            .assertions
            .iter()
            .any(|assertion| assertion.id.starts_with("invariant/bounds")),
        "the bound applies to a refusal and must be reported: {:#?}",
        refused.assertions
    );
}
