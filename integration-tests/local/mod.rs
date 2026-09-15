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
use sha2::Digest as _;

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
fn the_profile_the_documentation_teaches_with_actually_measures_the_fixture() {
    // `examples/custom-profile` is not decoration: `docs/profile-format.md` and
    // `examples/custom-profile/README.md` both tell a reader to run it, and both quote its
    // result. A profile quoted in documentation that no longer validates, or that has
    // stopped being conformant against the reference contract, teaches a mistake — and it
    // is exactly the kind of drift nobody notices, because nothing else reads it.
    //
    // It is measured through the fixture specification root rather than the real one, so
    // this test runs on a checkout with no sibling specification repository. The bundle
    // references no shared document, so the root it is resolved against makes no
    // difference to what it requires.
    let bundle = harness::repo_root().join("examples/custom-profile");
    assert!(
        bundle.join("profile.yaml").is_file(),
        "the example bundle has moved; {} no longer holds a profile",
        bundle.display()
    );
    let outcome = harness::run_bundle(&bundle, "none")
        .unwrap_or_else(|problem| panic!("the example bundle could not be executed: {problem}"));
    assert!(
        harness::failures(&outcome).is_empty(),
        "the example profile must hold against the reference contract; failures: {:#?}",
        harness::failures(&outcome)
    );
    assert_eq!(harness::status(&outcome), ConformanceStatus::Conformant);
    assert_eq!(
        harness::vector_ids(&outcome),
        vec!["balance-is-reported-exactly", "decimals-reports-the-scale"]
    );
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
fn a_profile_declaring_a_format_this_runner_cannot_execute_is_refused() {
    let bundle = harness::bundle_with("profile-wrong-spec-version");
    let problem = harness::run_bundle(&harness::bundle_root(&bundle), "none")
        .expect_err("a specification format that is not implemented must not be executed");
    assert_eq!(problem.class(), ErrorClass::ProfileError);
    assert_eq!(error_exit_code(&problem), ExitCode::SpecificationInvalid);
    assert!(
        problem.message().contains("99.0"),
        "the refusal must name the version it cannot read: {problem}"
    );
}

#[test]
fn a_profile_that_lists_a_document_it_does_not_have_is_refused() {
    // Tolerating this would evaluate a profile with no authorization requirements at all
    // and report a contract conformant against requirements nobody read.
    let bundle = harness::bundle_with("profile-missing-document");
    let problem = harness::run_bundle(&harness::bundle_root(&bundle), "none")
        .expect_err("a manifest that names an absent document must not be executed");
    assert_eq!(problem.class(), ErrorClass::ProfileError);
    assert_eq!(error_exit_code(&problem), ExitCode::SpecificationInvalid);
    assert!(
        problem.message().contains("authorization-absent.yaml"),
        "the refusal must name the document that is missing: {problem}"
    );
}

#[test]
fn a_cross_reference_that_does_not_resolve_is_an_error_and_not_a_warning() {
    // The requirement this rule states cannot be evaluated, so applying the profile
    // without it would report a verdict against something that was never applied — the
    // failure mode the whole project exists to prevent.
    let bundle = harness::bundle_with("profile-broken-reference");
    let problem = harness::run_bundle(&harness::bundle_root(&bundle), "none")
        .expect_err("a requirement naming something undeclared must not be executed");
    assert_eq!(problem.class(), ErrorClass::ProfileError);
    assert_eq!(error_exit_code(&problem), ExitCode::SpecificationInvalid);
}

#[test]
fn a_vector_with_no_expected_outcome_is_a_corpus_error() {
    // A different class from an unusable profile, and it has to be: the requirements are
    // readable, and the document that describes what to measure against them is not.
    let bundle = harness::bundle_with("vector-missing-field");
    let problem = harness::run_bundle(&harness::bundle_root(&bundle), "none")
        .expect_err("a vector with no expected outcome must not be executed");
    assert_eq!(problem.class(), ErrorClass::VectorError);
    assert_eq!(error_exit_code(&problem), ExitCode::SpecificationInvalid);
}

#[test]
fn a_valid_bundle_with_a_valid_overlay_is_still_a_valid_bundle() {
    // The control for the four tests above: they would all pass if copying the bundle
    // broke it. This one copies nothing and runs, so a failure in the four is
    // attributable to the document they added.
    let bundle = harness::bundle_with("profile-wrong-spec-version");
    let root = harness::bundle_root(&bundle);
    assert!(root.join("profile.yaml").is_file());
    assert!(
        root.join("vectors/transfer/transfer-moves-the-exact-amount.yaml")
            .is_file()
    );
    assert!(
        root.join("vectors/balance/balance-is-reported.yaml")
            .is_file()
    );
}

#[test]
fn a_stored_report_is_re_renderable_and_re_reads_identically() {
    // The artefact a receipt commits to and that another tool consumes. A stored report
    // that could not be read back is a verification that cannot be performed offline,
    // which is the only situation a receipt is for.
    let text = std::fs::read_to_string(harness::stored_report_path()).unwrap();
    let stored: estamora_report::Report = serde_json::from_str(&text)
        .unwrap_or_else(|problem| panic!("the stored report is not readable: {problem}"));
    assert_eq!(stored.status, ConformanceStatus::Conformant);
    assert_eq!(stored.exit_code, 0);
    assert_eq!(stored.profile.id, "conformance-token");
    assert_eq!(stored.vectors.count, 4);
    assert_eq!(stored.summary.total(), 59);

    let rendered = estamora_report::render_markdown(&stored).unwrap();
    assert!(rendered.contains("CONFORMANT"));
    assert!(rendered.contains("conformance-token@1.0"));
    let junit = estamora_report::render_junit(&stored).unwrap();
    assert!(junit.contains("tests=\"4\""));
}

#[test]
fn a_document_that_is_not_a_report_is_refused_rather_than_read_as_one() {
    let text =
        std::fs::read_to_string(harness::malformed_inputs_root().join("report-not-a-report.json"))
            .unwrap();
    let problem =
        estamora_report::parse(&text).expect_err("valid JSON is not a conformance report");
    // The class is the claim that matters, and it is the one the taxonomy reserves for a
    // document that is not what it was handed as. The exit code follows the published
    // contract rather than this test's preference: `REPORT_ERROR` is not a specification
    // problem, an environment problem or a verdict about a contract, so it maps to `5`
    // and a script must never read it as a non-conformant contract.
    assert_eq!(problem.class(), ErrorClass::ReportError);
    assert_eq!(error_exit_code(&problem), ExitCode::InternalError);
    assert_ne!(error_exit_code(&problem), ExitCode::NonConformant);
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

#[test]
fn a_compiled_artifact_is_deployed_and_read_from_its_own_spec_section() {
    // The `.wasm` target is the one a user meets first — it is what
    // `examples/local-contract` and `docs/local-testing.md` walk through — and until
    // there was an artifact to measure it with, nothing exercised it: the in-repository
    // fixtures are registered from their Rust types, so deploying real bytes and reading
    // an interface out of a real `contractspecv0` section were both untested.
    //
    // Inspection is what is asserted rather than a per-dimension tally, because a tally
    // belongs to a vector and no vector against an artifact can be decided. What this
    // establishes is the weaker, checkable half: the artifact deploys, and the methods
    // below were read out of its own bytes rather than out of a declaration beside them.
    let target =
        estamora_cli::engine::Target::parse(harness::fixture_wasm_path().to_str().unwrap(), None)
            .unwrap();
    let config = estamora_cli::RunConfig::new(
        harness::fixture_profile_root(),
        harness::fixture_spec_root(),
        target,
    );
    let inspection = estamora_cli::engine::inspect(&config).expect("a built artifact must deploy");

    assert_eq!(inspection.network, "local");
    assert!(
        inspection.wasm_hash.is_some(),
        "a deployed artifact has a hash, which is what pins a result to a compilation"
    );
    for method in [
        "allowance",
        "approve",
        "balance",
        "burn",
        "burn_from",
        "decimals",
        "name",
        "symbol",
        "transfer",
        "transfer_from",
    ] {
        assert!(
            inspection.interface.declares(method),
            "`{method}` was not read out of the artifact; the interface found was {} ({})",
            inspection.interface.methods.len(),
            inspection.interface.source
        );
    }
}

#[test]
fn the_digest_a_report_records_for_an_artifact_is_the_digest_of_its_bytes() {
    // The hash is what ties a result to the exact compilation it was reached from, so it
    // is computed here from the file rather than taken from the report. A digest of
    // something else — the path, the deployment's address, a cached value — would still
    // look like a digest and would pin nothing.
    let outcome = harness::run_artifact();
    let recorded = harness::report(&outcome)
        .target
        .wasm_hash
        .clone()
        .expect("a deployed artifact has a hash");

    let bytes = std::fs::read(harness::fixture_wasm_path()).unwrap();
    let computed = hex::encode(sha2::Sha256::digest(&bytes));

    assert_eq!(recorded, computed);
    // The form is part of the claim, not a presentation choice. The report schema
    // requires a bare 64-character code hash in this field, and a network states one in
    // the same form — so a prefix here would be both invalid and a second spelling of the
    // same deployment, differing by which route read it.
    assert_eq!(recorded.len(), 64, "a code hash is 64 hex characters");
    assert!(
        recorded
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "a code hash is lowercase hex: {recorded}"
    );
}

#[test]
fn an_artifact_whose_world_cannot_be_seeded_produces_no_verdict() {
    // No state can be put into a deployed artifact, so every vector is skipped and
    // nothing is decided. That is an environment failure and exits `4`, which is the
    // classification that matters most here: a run that could not prepare a single
    // scenario must not report a contract as non-conformant, and it must not report it
    // as conformant either. The failure mode this guards against is a suite whose
    // vectors all fail to run and which therefore finds nothing wrong.
    let outcome = harness::run_artifact();
    let report = harness::report(&outcome);

    assert_eq!(harness::status(&outcome), ConformanceStatus::ExecutionError);
    assert_ne!(harness::status(&outcome), ConformanceStatus::Conformant);
    assert_ne!(
        harness::status(&outcome),
        ConformanceStatus::NonConformant,
        "an environment that could not prepare a scenario is not a contract that failed"
    );
    assert_eq!(
        harness::status(&outcome).exit_code(),
        ExitCode::EnvironmentFailed
    );
    assert_eq!(report.vectors.count, 4);

    for vector in &outcome.outcomes {
        assert_eq!(
            vector.status,
            estamora_core::VectorStatus::Skipped,
            "`{}` was decided against an artifact whose state was never established",
            vector.vector_id
        );
        assert!(
            vector
                .diagnostics
                .iter()
                .any(|finding| finding.code == "seeding-unavailable"),
            "`{}` was skipped without saying why: {:#?}",
            vector.vector_id,
            vector.diagnostics
        );
    }
}
