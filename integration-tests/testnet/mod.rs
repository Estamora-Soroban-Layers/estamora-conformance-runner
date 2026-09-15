//! Network targets, end to end.
//!
//! A deployed contract is resolved over RPC to the WebAssembly it is running, and that
//! artifact is then measured in the local host. The tests here are not about whether a
//! network happens to be reachable — they are about how every way of *not* reading a
//! contract is reported. The failure mode this file exists to prevent is the one that
//! would be unacceptable: accepting a contract identifier, doing nothing with it, and
//! printing a verdict. A fabricated `CONFORMANT` is indistinguishable from a real one to
//! everyone who did not run it.
//!
//! So the partition asserted here is the load-bearing one. A contract that does not
//! exist, a network that cannot be reached, and a code hash that disagrees with the
//! artifact it claims to describe are all environment failures that exit `4`. None of
//! them may be reported as a contract that failed its profile, which is the only thing
//! exit `1` means and the only thing that may block a release.
//!
//! # Why reading a network is opt-in here
//!
//! The default suite runs on a checkout with no network, and a public testnet must not be
//! able to turn this repository red — a conformance runner that fails when somebody
//! else's system is down teaches its users to ignore it. The classification itself is
//! therefore asserted elsewhere, deterministically and without a network:
//! `estamora-soroban`'s own `tests/testnet.rs` points a known network name at a closed
//! port, and `estamora-cli`'s `engine::target` tests do the same through the CLI's own
//! fetch path. Both establish that an outage is an environment failure, which is the
//! property that matters. What is left here is the part that genuinely needs a live
//! ledger: whether a real deployment resolves to the artifact it declares. Those tests are
//! marked ignored and are run by `scripts/test-testnet.sh` and the `testnet` job.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]

use estamora_core::{ErrorClass, ExitCode};
use estamora_integration_tests as harness;

/// A well-formed contract identifier that does not exist on any network.
///
/// Well formed on purpose: a malformed one would be refused by the shape check before any
/// transport was reached, and would therefore prove nothing about how a network target is
/// reported.
const ABSENT_CONTRACT: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM";

/// Whether the network-backed tests were asked for.
///
/// `cargo test --workspace -- --ignored` is what this repository's CI runs to get at the
/// cross-repository tests, so a test that is merely `#[ignore]`d would still reach a public
/// node on every push. A shared ledger must not be able to turn this repository red, so
/// reaching one is a separate, explicit decision — the same one `scripts/test-testnet.sh`
/// makes — rather than a side effect of asking for the ignored set.
fn testnet_enabled() -> bool {
    std::env::var("ESTAMORA_TESTNET_ENABLED").is_ok_and(|value| value == "1")
}

/// Skips, loudly, when the network tests were not asked for.
///
/// Loudly rather than silently: a suite that reports success because it ran nothing is
/// worse than one that fails, and the whole point of the ignored set is that its absence
/// from a run is visible in the output.
macro_rules! require_testnet {
    () => {
        if !testnet_enabled() {
            eprintln!(
                "ESTAMORA_TESTNET_ENABLED is not 1; this test reads a live ledger and did \
                 nothing. Run scripts/test-testnet.sh, or set ESTAMORA_TESTNET_ENABLED=1."
            );
            return;
        }
    };
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
fn a_local_run_names_its_network_as_local() {
    // The counterpart of the network tests: a local run must not claim a network, because
    // a reader comparing two reports has to be able to tell which one measured a
    // deployment.
    let outcome = harness::run_fixture("none");
    assert_eq!(harness::report(&outcome).target.network, "local");
    assert!(
        harness::report(&outcome)
            .target
            .contract
            .starts_with("fixture:")
    );
}

#[test]
#[ignore = "reads a live ledger; run through scripts/test-testnet.sh"]
fn a_contract_that_does_not_exist_is_an_environment_failure_and_never_a_verdict() {
    require_testnet!();
    let problem = harness::run_target(ABSENT_CONTRACT, Some("testnet"))
        .expect_err("a contract that does not exist cannot produce a report");
    assert_eq!(
        problem.class(),
        ErrorClass::ContractResolutionError,
        "a missing contract is a resolution failure: {problem}"
    );
    assert_eq!(
        problem.context_value("reason"),
        Some("contract-not-found"),
        "the failure must name which resolution step failed: {problem}"
    );
    assert_eq!(problem.context_value("contract"), Some(ABSENT_CONTRACT));
    assert_eq!(problem.context_value("network"), Some("testnet"));
    assert_eq!(
        problem.blame(),
        estamora_core::Blame::Environment,
        "an absent contract says nothing about any contract's behaviour"
    );
    assert_ne!(
        estamora_cli::exit_code(problem.class()),
        ExitCode::NonConformant,
        "an unresolvable contract must never look like a non-conformant one"
    );
}

#[test]
#[ignore = "reads a live ledger; run through scripts/test-testnet.sh"]
fn a_deployment_is_measured_and_the_report_names_the_deployment_it_was_reached_about() {
    require_testnet!();
    // Named rather than discovered, because which contract is measured has to be a
    // decision somebody made. Skipped loudly when nothing is named, so that a run of the
    // ignored set either measures something or says why it could not.
    let Ok(contract) = std::env::var("ESTAMORA_TESTNET_CONTRACT") else {
        eprintln!(
            "ESTAMORA_TESTNET_CONTRACT does not name a deployed contract; nothing was \
             measured. A verdict without its target is not re-verifiable, which is the \
             property this test exists to assert."
        );
        return;
    };
    let outcome = harness::run_target(&contract, Some("testnet"))
        .unwrap_or_else(|problem| panic!("{contract} could not be read: {problem}"));

    let report = harness::report(&outcome);
    assert_eq!(report.target.contract, contract);
    assert_eq!(report.target.network, "testnet");

    // The verdict itself is about the profile, not about what the contract is. What must
    // hold regardless is that every requirement the corpus declares was either exercised
    // or reported as not exercised — a run may not be silent about a requirement it could
    // not reach.
    assert!(
        !outcome.outcomes.is_empty(),
        "a run that executed no vector is not a verdict"
    );
}
