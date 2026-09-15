//! State transitions, end to end.
//!
//! These tests are about what a contract did to its own storage, which is the part of a
//! conformance result a reader is most likely to trust without checking. A contract that
//! moves 250 and credits 249 leaves a ledger that balances to nobody's satisfaction, and
//! no event requirement and no interface check can see it.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]

use estamora_core::ConformanceStatus;
use estamora_integration_tests as harness;

#[test]
fn a_transfer_that_credits_less_than_it_debits_is_caught_by_the_state_assertions() {
    let outcome = harness::run_fixture("wrong-credit-amount");
    assert_eq!(
        harness::status(&outcome),
        ConformanceStatus::PartiallyConformant
    );
    harness::assert_failed(&outcome, "state/recipient-credited");
    // The sender's side of the movement is right, so it passes: the failure has to point
    // at the half of the movement that was wrong.
    harness::assert_passed(&outcome, "state/sender-debited");
}

#[test]
fn a_contract_that_permits_an_overdraft_is_caught_and_produces_a_negative_balance() {
    // The negative balance is the arithmetic signature of the missing check, and it
    // fails three separate requirements: the must-fail rule, the bound, and the state
    // assertions the vector declares. Any one of them would catch it; a report that
    // listed all three is what a reader needs to see how far the defect reaches.
    let outcome = harness::run_fixture("allows-overdraft");
    harness::assert_failed(&outcome, "state/sender-unchanged");
    harness::assert_failed(&outcome, "state/recipient-unchanged");
    harness::assert_failed(&outcome, "behavior/transfer-beyond-the-balance-fails");
}

#[test]
fn the_conforming_fixture_satisfies_every_state_assertion_it_is_measured_on() {
    let outcome = harness::run_fixture("none");
    harness::assert_passed(&outcome, "state/sender-debited");
    harness::assert_passed(&outcome, "state/recipient-credited");
    harness::assert_passed(&outcome, "state/balance-unchanged");
    let failed = harness::failures(&outcome);
    assert!(
        failed.iter().all(|check| !check.contains("state/")),
        "no state assertion should have failed: {failed:#?}"
    );
}

#[test]
fn a_read_that_left_its_own_subject_alone_is_reported_as_such() {
    // The read-only vector asserts the balance it read is unchanged. A contract that
    // returned the right number and rewrote the entry it read would satisfy the return
    // check alone, and this is the assertion that catches it.
    let outcome = harness::run_fixture_with_tags("none", &["read-only"]);
    assert_eq!(harness::vector_ids(&outcome), vec!["balance-is-reported"]);
    harness::assert_passed(&outcome, "state/balance-unchanged");
}
