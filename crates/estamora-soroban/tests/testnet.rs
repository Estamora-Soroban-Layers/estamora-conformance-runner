//! Resolution against a live network.
//!
//! These are `#[ignore]`d and take their target from the environment, because the default
//! suite must run on a checkout with no network and a public testnet must not be able to
//! turn this repository red. That is the whole reason they are separated out: a shared
//! ledger is somebody else's system, and a conformance runner that fails when somebody
//! else's system is down teaches its users to ignore it.
//!
//! Run them with:
//!
//! ```console
//! $ ESTAMORA_TESTNET_CONTRACT=C... cargo test -p estamora-soroban --test testnet -- --ignored
//! ```
//!
//! # What they establish, and what they do not
//!
//! They establish that a deployed contract can be resolved to its artifact, that the
//! artifact is verified against the hash the contract instance declares, and that the
//! failures around resolution are classified as environment or usage problems rather
//! than as anything about a contract's behaviour. They say nothing whatever about
//! whether the contract is conformant: that is what a profile and a corpus are for, and
//! it is decided locally.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]

use estamora_core::{Blame, ErrorClass};

/// The contract to resolve, from the environment.
///
/// `None` when the variable is unset, which makes the test report that it had nothing to
/// do rather than passing as though it had done something.
fn target() -> Option<(String, String)> {
    let contract = std::env::var("ESTAMORA_TESTNET_CONTRACT").ok()?;
    let network =
        std::env::var("ESTAMORA_TESTNET_NETWORK").unwrap_or_else(|_| "testnet".to_owned());
    Some((contract, network))
}

/// Skips, loudly, when the environment does not name a contract.
///
/// The suppression is stated rather than left to chance: `print_stderr` is denied
/// workspace-wide so that the runner cannot write alongside its own structured output,
/// and a test reporting why it stood down is not that. It is written out even though the
/// lint does not currently look inside a macro body, because passing by accident is not
/// the same as passing.
#[allow(
    clippy::print_stderr,
    reason = "a test that stands down has to say so in the test runner's output, and it is \
              not the CLI's structured output that the lint protects"
)]
macro_rules! target_or_skip {
    () => {
        match target() {
            Some(target) => target,
            None => {
                eprintln!("ESTAMORA_TESTNET_CONTRACT is unset; nothing to resolve");
                return;
            },
        }
    };
}

#[test]
#[ignore = "needs a network; set ESTAMORA_TESTNET_CONTRACT and pass --ignored"]
fn a_deployed_contract_resolves_to_code_that_matches_its_own_declared_hash() {
    let (contract, network) = target_or_skip!();

    let resolved = estamora_soroban::resolve(&contract, &network, None)
        .expect("a deployed contract must resolve");

    assert_eq!(resolved.contract_id, contract);
    assert_eq!(resolved.network, network);
    assert!(
        !resolved.passphrase.is_empty(),
        "the passphrase must be reported"
    );
    assert!(
        resolved.protocol_version > 0,
        "a protocol version must be reported"
    );

    // The hash is the 64-character hex of a 32-byte digest, and `resolve` has already
    // checked that the bytes it fetched hash to it. Re-checking here would only restate
    // the implementation, so what is asserted is that the artifact is a wasm module:
    // the magic number is the one thing the ledger cannot get wrong by disagreeing with
    // itself.
    assert_eq!(resolved.wasm_hash.len(), 64);
    assert_eq!(
        &resolved.wasm[..4],
        b"\0asm",
        "the resolved artifact must be a WebAssembly module"
    );
}

#[test]
#[ignore = "needs a network; set ESTAMORA_TESTNET_CONTRACT and pass --ignored"]
fn a_resolved_artifact_is_measurable_without_the_network() {
    let (contract, network) = target_or_skip!();

    let resolved = estamora_soroban::resolve(&contract, &network, None)
        .expect("a deployed contract must resolve");

    // This is the property the architecture rests on: what the network contributed is
    // bytes, and everything after this point is local. Reading the spec section is the
    // first thing a local run does, so if it works here then a resolved contract can be
    // measured exactly like a `.wasm` artifact on disk.
    let interface = estamora_soroban::ExposedInterface::from_wasm(&resolved.wasm, "resolved")
        .expect("a deployed contract must publish a contract spec");

    assert!(
        !interface.methods.is_empty(),
        "a contract with an empty spec section cannot be measured against any profile"
    );
}

#[test]
#[ignore = "needs a network; set ESTAMORA_TESTNET_CONTRACT and pass --ignored"]
fn an_unknown_network_names_the_ones_that_are_known() {
    let (contract, _) = target_or_skip!();

    let problem = estamora_soroban::resolve(&contract, "not-a-network", None).unwrap_err();

    assert_eq!(problem.class(), ErrorClass::UsageError);
    assert_eq!(problem.blame(), Blame::Invocation);
    assert!(
        problem.message().contains("testnet"),
        "{}",
        problem.message()
    );
}

#[test]
#[ignore = "needs a network; set ESTAMORA_TESTNET_CONTRACT and pass --ignored"]
fn a_contract_that_does_not_exist_is_not_reported_as_a_contract_defect() {
    let (_, network) = target_or_skip!();

    // A well-formed contract identifier that nothing is stored at.
    let absent = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM";
    let problem = estamora_soroban::resolve(absent, &network, None).unwrap_err();

    assert_eq!(problem.class(), ErrorClass::ContractResolutionError);
    assert_eq!(
        problem.blame(),
        Blame::Environment,
        "not being able to read a contract says nothing about how it behaves"
    );
    assert_eq!(
        problem.context_value("reason"),
        Some("contract-not-found"),
        "a missing contract must be distinguishable from an unreachable network"
    );
}

#[test]
#[ignore = "needs a network; set ESTAMORA_TESTNET_CONTRACT and pass --ignored"]
fn an_unreachable_endpoint_is_an_environment_failure_and_never_a_verdict() {
    let (contract, _) = target_or_skip!();

    // A routable address with nothing listening. The failure must be a network failure
    // that names the endpoint, because a runner that reported this as a contract defect
    // would block releases on somebody else's outage.
    let problem =
        estamora_soroban::resolve(&contract, "testnet", Some("http://127.0.0.1:1")).unwrap_err();

    assert_eq!(problem.class(), ErrorClass::NetworkError);
    assert_eq!(problem.blame(), Blame::Environment);
    assert_eq!(problem.context_value("reason"), Some("network-unreachable"));
    assert_eq!(
        problem.context_value("rpc_url"),
        Some("http://127.0.0.1:1"),
        "the failure must name the endpoint that was unreachable"
    );
}
