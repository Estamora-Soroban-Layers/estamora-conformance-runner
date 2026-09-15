//! Checking a produced report against the schema the specification publishes.
//!
//! The report model mirrors `schema/report.schema.json`, and a model that mirrors
//! something is a claim about it. This is the test that keeps the claim honest: it
//! takes a report this crate actually produces and validates it against the schema
//! the specification repository actually ships, rather than against a copy that could
//! drift.
//!
//! It needs a checkout of `estamora-conformance-spec`, so it is ignored by default
//! and runs from CI, where both repositories are present.
//!
//! # Resolving the cross-file reference
//!
//! The report schema refers to `profile.schema.json#/$defs/…` for four definitions.
//! Rather than depend on a resolver's treatment of relative references, the two
//! documents are merged here: the profile schema's definitions are copied into the
//! report schema's, and each cross-file reference is rewritten to a local one. The
//! merge is asserted rather than assumed — a reference that no longer resolves fails
//! the test instead of silently validating nothing.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use estamora_core::ConformanceStatus;
use estamora_report::model::{
    CorpusIdentity, ProfileIdentity, Report, ReportedAssertion, ReportedDiagnostic, RunnerIdentity,
    Summary, Target, VectorResult,
};

/// The checkout of the specification repository, when there is one.
fn spec_root() -> Option<PathBuf> {
    let root = std::env::var_os("ESTAMORA_SPEC_REPO")?;
    let root = PathBuf::from(root);
    root.is_dir().then_some(root)
}

/// The report schema with its cross-file references resolved.
fn resolved_report_schema(root: &std::path::Path) -> serde_json::Value {
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("schema/report.schema.json")).unwrap(),
    )
    .unwrap();
    let profile: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("schema/profile.schema.json")).unwrap(),
    )
    .unwrap();

    let mut merged = report;
    let definitions = merged["$defs"].as_object_mut().unwrap();
    for (name, definition) in profile["$defs"].as_object().unwrap() {
        definitions.insert(name.clone(), definition.clone());
    }

    // Rewrite every `profile.schema.json#/$defs/name` to `#/$defs/name`.
    let text = serde_json::to_string(&merged).unwrap();
    let rewritten = text.replace("profile.schema.json#/$defs/", "#/$defs/");
    assert_ne!(
        text, rewritten,
        "the report schema no longer refers to the profile schema, so this test would \
         validate less than it claims"
    );
    serde_json::from_str(&rewritten).unwrap()
}

/// A report with a passing and an undecidable vector, and every optional field set.
fn a_report() -> Report {
    Report {
        schema: Some(estamora_report::REPORT_SCHEMA.to_owned()),
        estamora_spec_version: "1.0".to_owned(),
        runner: RunnerIdentity {
            name: "estamora".to_owned(),
            version: "0.1.0".to_owned(),
        },
        generated_at: "2026-01-15T12:00:00Z".to_owned(),
        target: Target {
            contract: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM".to_owned(),
            network: "local".to_owned(),
            // Reported as null rather than omitted when it could not be resolved, so
            // that the absence is visible.
            wasm_hash: None,
            metadata: BTreeMap::from([(
                "name".to_owned(),
                serde_json::Value::String("A hostile name \"/>".to_owned()),
            )]),
        },
        profile: ProfileIdentity {
            id: "sep-41".to_owned(),
            version: "1.0".to_owned(),
            digest: format!("sha256:{}", "a".repeat(64)),
        },
        vectors: CorpusIdentity {
            digest: format!("sha256:{}", "b".repeat(64)),
            count: 2,
        },
        configuration: BTreeMap::from([(
            "network".to_owned(),
            serde_json::Value::String("local".to_owned()),
        )]),
        results: vec![
            VectorResult {
                vector_id: "transfer-moves-exact-amount".to_owned(),
                category: "positive".to_owned(),
                status: "passed".to_owned(),
                assertions: vec![ReportedAssertion {
                    id: "state-sender-debited".to_owned(),
                    category: "state".to_owned(),
                    status: "passed".to_owned(),
                    expected: "750".to_owned(),
                    observed: "750".to_owned(),
                    detail: Some("check: state/sender-debited".to_owned()),
                }],
                diagnostics: Vec::new(),
            },
            VectorResult {
                vector_id: "transfer-without-signature-fails".to_owned(),
                category: "authorization".to_owned(),
                status: "error".to_owned(),
                assertions: Vec::new(),
                diagnostics: vec![ReportedDiagnostic {
                    code: "interface-unavailable".to_owned(),
                    message: "the interface could not be read".to_owned(),
                }],
            },
        ],
        summary: Summary::default(),
        status: ConformanceStatus::Inconclusive,
        exit_code: 2,
    }
}

#[test]
#[ignore = "needs a checkout of estamora-conformance-spec; set ESTAMORA_SPEC_REPO and pass --ignored"]
fn a_produced_report_satisfies_the_published_schema() {
    let Some(root) = spec_root() else {
        return;
    };
    let schema = resolved_report_schema(&root);
    let validator = jsonschema::validator_for(&schema).unwrap();

    let report = a_report();
    let document = serde_json::to_value(&report).unwrap();

    let errors: Vec<String> = validator
        .iter_errors(&document)
        .map(|error| format!("{} at {}", error, error.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "a report this crate produces must satisfy the specification's schema:\n{}",
        errors.join("\n")
    );
}

#[test]
#[ignore = "needs a checkout of estamora-conformance-spec; set ESTAMORA_SPEC_REPO and pass --ignored"]
fn the_schema_rejects_a_report_whose_status_is_not_one_of_the_six() {
    // The negative case matters more than the positive one: a schema that accepted
    // anything would make the positive test vacuous, and a runner that could report a
    // seventh status would break every consumer that matches on the six.
    let Some(root) = spec_root() else {
        return;
    };
    let schema = resolved_report_schema(&root);
    let validator = jsonschema::validator_for(&schema).unwrap();

    let mut document = serde_json::to_value(a_report()).unwrap();
    document["status"] = serde_json::Value::String("PROBABLY_FINE".to_owned());
    assert!(
        !validator.is_valid(&document),
        "a status outside the six must be refused"
    );

    let mut document = serde_json::to_value(a_report()).unwrap();
    document["results"][0]["assertions"][0]["status"] =
        serde_json::Value::String("maybe".to_owned());
    assert!(
        !validator.is_valid(&document),
        "an assertion status outside passed/failed must be refused"
    );
}
