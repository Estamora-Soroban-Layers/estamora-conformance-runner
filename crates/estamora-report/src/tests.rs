//! The three renderings are derived from one document.
//!
//! A disagreement between them would be a defect rather than a difference of opinion,
//! so the tests here check the properties a reader relies on no matter which
//! rendering they are handed: the verdict is stated once and identically, a failure
//! is visible, and the limitation is not omitted.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]

use std::collections::BTreeMap;

use estamora_core::ConformanceStatus;

use crate::model::{
    CorpusIdentity, ProfileIdentity, Report, ReportedAssertion, RunnerIdentity, Summary, Target,
    VectorResult,
};

/// A report with one failing vector.
fn a_failing_report() -> Report {
    let assertion = ReportedAssertion {
        id: "state-sender-debited".to_owned(),
        category: "state".to_owned(),
        status: "failed".to_owned(),
        expected: "750".to_owned(),
        observed: "500".to_owned(),
        detail: None,
    };
    Report {
        schema: Some(crate::model::REPORT_SCHEMA.to_owned()),
        estamora_spec_version: "1.0".to_owned(),
        runner: RunnerIdentity {
            name: "estamora".to_owned(),
            version: "0.1.0".to_owned(),
        },
        generated_at: "2026-01-15T12:00:00Z".to_owned(),
        target: Target {
            contract: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM".to_owned(),
            network: "local".to_owned(),
            wasm_hash: None,
            metadata: BTreeMap::new(),
        },
        profile: ProfileIdentity {
            id: "sep-41".to_owned(),
            version: "1.0".to_owned(),
            digest: format!("sha256:{}", "a".repeat(64)),
        },
        vectors: CorpusIdentity {
            digest: format!("sha256:{}", "b".repeat(64)),
            count: 20,
        },
        configuration: BTreeMap::new(),
        results: vec![VectorResult {
            vector_id: "transfer-moves-exact-amount".to_owned(),
            category: "positive".to_owned(),
            status: "failed".to_owned(),
            assertions: vec![assertion],
            diagnostics: Vec::new(),
        }],
        summary: Summary::default(),
        status: ConformanceStatus::NonConformant,
        exit_code: 1,
    }
}

#[test]
fn every_rendering_states_the_verdict_it_is_given() {
    let report = a_failing_report();
    let json = crate::render_json(&report, false).unwrap();
    let markdown = crate::render_markdown(&report).unwrap();
    let junit = crate::render_junit(&report).unwrap();

    assert!(json.contains("NON_CONFORMANT"));
    assert!(markdown.contains("NON_CONFORMANT"));
    assert!(junit.contains("failed"));
}

#[test]
fn every_rendering_says_what_it_does_not_establish() {
    // The Markdown rendering is the one a person reads, so the limitation is stated
    // there in prose. JSON and JUnit carry it structurally: the status is one of six
    // explicit values rather than a boolean, and an undecidable vector is an error
    // rather than a failure.
    let markdown = crate::render_markdown(&a_failing_report()).unwrap();
    assert!(markdown.contains("not a security audit"), "{markdown}");
    assert!(markdown.contains("does not prove the absence of vulnerabilities"));
}

#[test]
fn the_compact_json_is_what_a_receipt_would_digest() {
    let report = a_failing_report();
    let compact = crate::render_json(&report, false).unwrap();
    let parsed = crate::parse(&compact).unwrap();
    assert_eq!(parsed, report);
    assert_eq!(compact, crate::render_json(&report, false).unwrap());
}

#[test]
fn a_check_that_failed_anywhere_is_summarised_as_failed() {
    // Found by running the real corpus against the conforming fixture. Behaviour rules
    // are evaluated once per vector, so one identifier appears in many results, and the
    // summary counts it once. Counting the *first* result let a requirement one
    // scenario violated be reported as satisfied because another scenario had satisfied
    // it — a summary stating the opposite of what the run found.
    let assertion = |status: &str| ReportedAssertion {
        id: "behavior-transfer-moves-exact-amount-postcondition-0".to_owned(),
        category: "behavior".to_owned(),
        status: status.to_owned(),
        expected: "decrease by 250".to_owned(),
        observed: "1000 -> 750".to_owned(),
        detail: None,
    };
    let result = |id: &str, status: &str| VectorResult {
        vector_id: id.to_owned(),
        category: "positive".to_owned(),
        status: status.to_owned(),
        assertions: vec![assertion(status)],
        diagnostics: Vec::new(),
    };

    // Passing first, failing second: the order must not decide the answer.
    let summary = Summary::of(&[result("passes", "passed"), result("fails", "failed")]);
    assert_eq!(summary.behavior.total, 1, "one identifier is one check");
    assert_eq!(summary.behavior.failed, 1);
    assert_eq!(summary.behavior.passed, 0);

    // And the other way round, which is the order the old implementation survived.
    let reversed = Summary::of(&[result("fails", "failed"), result("passes", "passed")]);
    assert_eq!(reversed.behavior.failed, 1);
    assert_eq!(reversed.behavior.passed, 0);

    // A check that held everywhere is still counted once, as a pass.
    let held = Summary::of(&[result("one", "passed"), result("two", "passed")]);
    assert_eq!(held.behavior.total, 1);
    assert_eq!(held.behavior.passed, 1);
    assert_eq!(held.behavior.failed, 0);
}

#[test]
fn an_undeclared_dimension_is_named_rather_than_guessed() {
    assert!(crate::model::category_of("interface").is_some());
    assert!(crate::model::category_of("security").is_none());
}
