//! Network targets, end to end.
//!
//! This build links no Soroban RPC transport, so a deployed contract cannot be measured.
//! The tests here are not about that limitation being acceptable — they are about the way
//! it is reported. The failure mode that would be unacceptable is the one this file
//! exists to prevent: accepting a contract identifier, doing nothing with it, and printing
//! a verdict. A fabricated `CONFORMANT` is indistinguishable from a real one to everyone
//! who did not run it.
//!
//! The opt-in run at the bottom is where a transport would be exercised, and it is written
//! so that it asserts the same two things a real one must: that a network *failure* is
//! never reported as a contract failure, and that a conformance verdict from a network run
//! names the network and the contract it was reached about.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]

use estamora_core::{ErrorClass, ExitCode};
use estamora_integration_tests as harness;

/// A well-formed contract identifier that does not exist on any network.
const ABSENT_CONTRACT: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM";

#[test]
fn a_deployed_contract_is_refused_rather_than_measured_against_nothing() {
    let outcome = harness::run_target(ABSENT_CONTRACT, Some("testnet"));
    let problem = outcome.expect_err("this build cannot reach a network");
    assert_eq!(problem.class(), ErrorClass::ContractResolutionError);
    assert_eq!(
        problem.context_value("reason"),
        Some("network-transport-unavailable"),
        "the refusal must name what is missing: {problem}"
    );
    assert_eq!(problem.context_value("contract"), Some(ABSENT_CONTRACT));
    assert_eq!(problem.context_value("network"), Some("testnet"));
}

#[test]
fn a_network_failure_is_never_reported_as_a_contract_failure() {
    // The single most damaging classification error this system could make: a CI job that
    // blocks a release because a network was unreachable, and that teaches its users to
    // distrust every verdict it produces. Exit `4` is an environment failure and exit `1`
    // is a contract that violated a requirement, and only a violated requirement may
    // produce the second.
    let problem = harness::run_target(ABSENT_CONTRACT, Some("testnet"))
        .expect_err("this build cannot reach a network");
    assert_eq!(problem.blame(), estamora_core::Blame::Environment);
    assert_eq!(
        estamora_cli::exit_code(problem.class()),
        ExitCode::EnvironmentFailed
    );
    assert_ne!(
        estamora_cli::exit_code(problem.class()),
        ExitCode::NonConformant,
        "an unreachable network must never look like a non-conformant contract"
    );
}

#[test]
fn a_contract_identifier_without_a_network_is_a_usage_error() {
    // Read as a target and failed later, this would report a command-line mistake as a
    // resolution outcome that CI treats as infrastructure.
    let problem = harness::run_target(ABSENT_CONTRACT, None)
        .expect_err("a deployed contract must say where it lives");
    assert_eq!(problem.class(), ErrorClass::UsageError);
    assert_eq!(estamora_cli::exit_code(problem.class()), ExitCode::Usage);
}

#[test]
fn something_that_is_not_a_contract_identifier_is_refused_before_any_transport_is_reached() {
    let problem = harness::run_target("transfer", Some("testnet"))
        .expect_err("a word is not a contract identifier");
    assert_eq!(problem.class(), ErrorClass::UsageError);
    assert!(problem.message().contains("56 characters"), "{problem}");
}

#[test]
fn a_testnet_run_is_attempted_only_when_one_is_configured() {
    // The suite must not depend on the network being reachable, so this test measures what
    // it can measure locally — that a network target is refused with the transport named —
    // and adds a real run only when a contract has been named for it. Without
    // `ESTAMORA_TESTNET_CONTRACT` it is the refusal that is asserted, which is the same
    // assertion the test above makes and is therefore never a silently skipped test.
    let Some(contract) = std::env::var_os("ESTAMORA_TESTNET_CONTRACT") else {
        let problem = harness::run_target(ABSENT_CONTRACT, Some("testnet"))
            .expect_err("no transport is linked, so no network run is possible");
        assert_eq!(
            problem.context_value("reason"),
            Some("network-transport-unavailable")
        );
        return;
    };
    let contract = contract.to_string_lossy().into_owned();
    match harness::run_target(&contract, Some("testnet")) {
        // A run reached a verdict, so the report must say which deployment it was
        // reached about. A verdict without its target is not re-verifiable.
        Ok(outcome) => {
            assert_eq!(harness::report(&outcome).target.contract, contract);
            assert_eq!(harness::report(&outcome).target.network, "testnet");
        },
        // Or this build cannot reach a network at all, which is the same refusal the
        // tests above assert and must not be reported as anything else.
        Err(problem) => assert_eq!(
            problem.context_value("reason"),
            Some("network-transport-unavailable"),
            "a network run that could not be attempted must name the missing transport: {problem}"
        ),
    }
}

#[test]
fn a_local_run_names_its_network_as_local() {
    // The counterpart of the test above: a local run must not claim a network, because a
    // reader comparing two reports has to be able to tell which one measured a deployment.
    let outcome = harness::run_fixture("none");
    assert_eq!(harness::report(&outcome).target.network, "local");
    assert!(
        harness::report(&outcome)
            .target
            .contract
            .starts_with("fixture:")
    );
}
