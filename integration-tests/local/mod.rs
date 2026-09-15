//! A whole run, from a profile on disk to a verdict.
//!
//! These are the tests that fail when the pipeline is broken rather than when a rule
//! is evaluated wrongly: resolution, seeding, invocation, observation, evaluation and
//! reporting all have to work for a single assertion here to mean anything. Each one
//! checks the part of the answer a caller acts on — the verdict, the exit code, the
//! report document, the corpus that was actually executed.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]

use estamora_core::{ConformanceStatus, Error, ErrorClass, ExitCode};
use estamora_integration_tests as harness;

/// The exit code a failed run produces, as the binary would.
fn error_exit_code(problem: &Error) -> ExitCode {
    estamora_cli::exit_code(problem.class())
}

#[test]
fn the_conforming_fixture_is_conformant_and_the_run_succeeds() {
    let outcome = harness::run_fixture("none");
    assert!(
        harness::failures(&outcome).is_empty(),
        "the reference contract must satisfy its own profile; failures: {:#?}",
        harness::failures(&outcome)
    );
    assert_eq!(harness::status(&outcome), ConformanceStatus::Conformant);
    assert_eq!(harness::status(&outcome).exit_code(), ExitCode::Success);
}

#[test]
fn every_vector_the_corpus_declares_is_executed() {
    // A run that silently dropped a vector would report CONFORMANT over a smaller
    // corpus than the profile describes, and nothing about the report would show it.
    let outcome = harness::run_fixture("none");
    let executed = harness::vector_ids(&outcome);
    let mut sorted = executed.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        vec![
            "balance-is-reported",
            "transfer-beyond-the-balance-fails",
            "transfer-moves-the-exact-amount",
            "transfer-without-authorization-fails",
        ]
    );
    assert_eq!(harness::report(&outcome).vectors.count, 4);
}

#[test]
fn all_seven_dimensions_are_reported_for_a_run() {
    // The failure this guards against is a dimension that reports nothing and so makes a
    // passing vector look like a vector that was never evaluated. The fixture profile is
    // built to exercise all seven, so every tally must be non-zero.
    let outcome = harness::run_fixture("none");
    let summary = &harness::report(&outcome).summary;
    for (dimension, tally) in summary.all() {
        assert!(
            tally.total > 0,
            "the {} dimension reported no checks at all",
            dimension.as_str()
        );
        assert_eq!(tally.failed, 0, "{} failed", dimension.as_str());
    }
    assert!(summary.total() >= 50, "the corpus is smaller than expected");
}

#[test]
fn the_run_records_the_profile_it_measured_and_the_digest_of_what_it_read() {
    // A verdict is only interpretable against the requirements it was reached from, so
    // the report has to name both the identity of the profile and the revision of its
    // content. A digest that did not change when the bundle changed would let two
    // different requirement sets be reported under one identity.
    let outcome = harness::run_fixture("none");
    let report = harness::report(&outcome);
    assert_eq!(report.profile.id, "conformance-token");
    assert_eq!(report.profile.version, "1.0");
    assert!(report.profile.digest.starts_with("sha256:"));
    assert!(report.vectors.digest.starts_with("sha256:"));
    assert_eq!(report.runner.name, "estamora");
    assert!(!report.runner.version.is_empty());
    assert!(!report.generated_at.is_empty());
    assert_eq!(report.target.contract, "fixture:none");
    assert_eq!(report.target.network, "local");
}

#[test]
fn the_report_is_the_document_the_schema_defines() {
    // The normative artefact is the JSON document, so it has to round-trip: another
    // implementation reading it must arrive at the same report this one wrote.
    let outcome = harness::run_fixture("none");
    let rendered = harness::json(&outcome);
    let parsed: estamora_report::Report = serde_json::from_str(&rendered)
        .unwrap_or_else(|problem| panic!("the report is not readable: {problem}"));
    assert_eq!(&parsed, harness::report(&outcome));
    // The verdict is a field of the document rather than something a consumer derives
    // from a count, so it is asserted on the parsed value and not on the rendering: the
    // spelling of the document is the schema's business, its content is the runner's.
    let document: serde_json::Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(document["status"], serde_json::json!("CONFORMANT"));
    assert_eq!(document["exit_code"], serde_json::json!(0));
}

#[test]
fn a_tag_filter_measures_only_the_vectors_it_selects() {
    let outcome = harness::run_fixture_with_tags("none", &["balance"]);
    assert_eq!(harness::vector_ids(&outcome), vec!["balance-is-reported"]);
    assert_eq!(harness::report(&outcome).vectors.count, 1);
    assert_eq!(harness::status(&outcome), ConformanceStatus::Conformant);
}

#[test]
fn a_tag_filter_that_selects_nothing_is_reported_rather_than_passing_quietly() {
    // The dangerous outcome is not an error: it is a run that measured nothing and said
    // CONFORMANT. The verdict must be undecided, and the reason must be recorded.
    let outcome = harness::run_fixture_with_tags("none", &["no-vector-carries-this-tag"]);
    assert!(
        !outcome
            .outcomes
            .iter()
            .any(|vector| !vector.assertions.is_empty()),
        "no vector should have been measured"
    );
    assert!(
        outcome
            .findings
            .iter()
            .any(|finding| finding.message.contains("nothing was executed")),
        "the empty selection must be recorded: {:#?}",
        outcome.findings
    );
    assert!(
        !harness::status(&outcome).describes_contract(),
        "a run that measured nothing must not report a verdict about the contract"
    );
}

#[test]
fn a_compiled_artifact_that_is_not_there_is_a_resolution_failure_and_not_a_verdict() {
    // The distinction CI depends on: the command line was well formed and the thing it
    // named was not there, which is an environment problem rather than a contract one.
    let target = estamora_cli::engine::Target::parse("/nowhere/conformance-token.wasm", None)
        .expect_err("a missing artifact is not a target");
    assert_eq!(target.class(), ErrorClass::ContractResolutionError);
    assert_eq!(target.blame(), estamora_core::Blame::Environment);
}

#[test]
fn an_unknown_fixture_is_refused_by_name_rather_than_defaulted() {
    // Defaulting to the conforming fixture would turn a typo into a green run against a
    // contract the caller never named.
    let problem = estamora_cli::engine::Target::parse("fixture:conformant", None)
        .expect_err("there is no fixture by that name");
    assert_eq!(problem.class(), ErrorClass::UsageError);
    assert!(
        problem.message().contains("omits-event"),
        "the refusal must name the fixtures that do exist: {}",
        problem.message()
    );
}

#[test]
fn a_bundle_that_does_not_resolve_is_a_profile_error_rather_than_an_empty_run() {
    let problem = harness::run_bundle(
        &harness::repo_root().join("fixtures/profiles/absent"),
        "none",
    )
    .expect_err("a bundle that is not there cannot be loaded");
    assert_eq!(problem.class(), ErrorClass::ProfileError);
    assert_eq!(problem.blame(), estamora_core::Blame::Specification);
    // Exit `3`, not `1`: the requirements were unusable, so no verdict about a contract
    // was reached and CI must not read this as a contract that failed its profile.
    assert_eq!(error_exit_code(&problem), ExitCode::SpecificationInvalid);
}

#[test]
fn the_sep_41_profile_is_conformant_against_the_fixture_when_the_specification_is_available() {
    // The cross-repository half of the same question, run only where the specification
    // repository has been checked out beside this one. It is the test that would catch a
    // change that made the runner agree with its own fixtures and disagree with SEP-41.
    let Some(spec) = std::env::var_os(estamora_cli::DEFAULT_SPEC_ROOT_ENV) else {
        return;
    };
    let spec = std::path::PathBuf::from(spec);
    if !spec.join("profiles/sep-41/1.0").is_dir() {
        return;
    }
    let target = estamora_cli::engine::Target::parse("fixture:none", None).unwrap();
    let config = estamora_cli::RunConfig::new(spec.join("profiles/sep-41/1.0"), spec, target);
    let outcome = estamora_cli::run(&config)
        .unwrap_or_else(|problem| panic!("the SEP-41 profile could not be executed: {problem}"));
    assert!(
        harness::failures(&outcome).is_empty(),
        "the reference contract must conform to SEP-41; failures: {:#?}",
        harness::failures(&outcome)
    );
    assert_eq!(harness::status(&outcome), ConformanceStatus::Conformant);
}
