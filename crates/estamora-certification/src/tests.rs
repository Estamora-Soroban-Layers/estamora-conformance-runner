//! End-to-end certification: a report, a receipt, a signature, a verifier.
//!
//! The fixture is shared with the other modules' tests so that every one of them
//! commits to the same document. A test that built its own report would prove that a
//! function agrees with itself.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]

use std::collections::BTreeMap;

use estamora_core::{ConformanceStatus, Result};
use estamora_report::model::{
    CorpusIdentity, ProfileIdentity, Report, ReportedAssertion, RunnerIdentity, Summary, Target,
    VectorResult,
};

/// A report with one failing vector and every dimension tallied.
pub fn a_report() -> Report {
    let assertion = ReportedAssertion {
        id: "state-sender-debited".to_owned(),
        category: "state".to_owned(),
        status: "failed".to_owned(),
        expected: "750".to_owned(),
        observed: "500".to_owned(),
        detail: Some("check: state/sender-debited".to_owned()),
    };
    let mut summary = Summary::default();
    summary.state.total = 1;
    summary.state.failed = 1;
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
            wasm_hash: Some("a".repeat(64)),
            metadata: BTreeMap::new(),
        },
        profile: ProfileIdentity {
            id: "sep-41".to_owned(),
            version: "1.0".to_owned(),
            digest: format!("sha256:{}", "c".repeat(64)),
        },
        vectors: CorpusIdentity {
            digest: format!("sha256:{}", "d".repeat(64)),
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
        summary,
        status: ConformanceStatus::NonConformant,
        exit_code: 1,
    }
}

/// The digest a receipt commits to is the digest of the report as JSON.
fn report_digest_is_stable() -> Result<()> {
    let report = a_report();
    let first = crate::report_digest(&report)?;
    let second = crate::report_digest(&report)?;
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn a_round_trip_from_report_to_receipt_to_verification() {
    let report = a_report();
    let key = crate::signing_key_from_hex(&"33".repeat(32)).unwrap();

    let receipt = crate::Receipt::issue(&report, "2026-01-15T12:00:01Z").unwrap();
    let signed = crate::sign(&receipt, &key).unwrap();

    let verification = crate::verify(&signed, &report, Some(&key.verifying_key())).unwrap();
    assert!(verification.is_attributed());
    assert_eq!(verification.report_digest, receipt.report_digest);
    assert_eq!(receipt.result.status, ConformanceStatus::NonConformant);
    assert_eq!(receipt.result.exit_code, 1);
}

#[test]
fn a_receipt_survives_being_written_down_and_read_back() {
    // A receipt's whole purpose is to be carried somewhere else, so the round trip
    // through JSON is part of its contract rather than an implementation detail.
    let report = a_report();
    let key = crate::signing_key_from_hex(&"44".repeat(32)).unwrap();
    let signed = crate::sign(
        &crate::Receipt::issue(&report, "2026-01-15T12:00:00Z").unwrap(),
        &key,
    )
    .unwrap();

    let text = serde_json::to_string(&signed).unwrap();
    let read: crate::SignedReceipt = serde_json::from_str(&text).unwrap();
    assert_eq!(read, signed);
    assert!(crate::verify(&read, &report, Some(&key.verifying_key())).is_ok());
}

#[test]
fn a_receipt_cannot_be_edited_into_a_better_verdict() {
    // The attack a receipt exists to prevent: take a non-conformant result, change
    // the verdict, and present the receipt with the report it no longer describes.
    let report = a_report();
    let key = crate::signing_key_from_hex(&"55".repeat(32)).unwrap();
    let mut signed = crate::sign(
        &crate::Receipt::issue(&report, "2026-01-15T12:00:00Z").unwrap(),
        &key,
    )
    .unwrap();

    signed.receipt.result.status = ConformanceStatus::Conformant;
    signed.receipt.result.exit_code = 0;

    // The receipt's own digest no longer matches what was signed, so the signature
    // fails — *and* if the signature were recomputed, the report digest check would
    // still catch it, because the report it is shown alongside did not change.
    let problem = crate::verify(&signed, &report, Some(&key.verifying_key())).unwrap_err();
    assert_eq!(
        problem.context_value("reason"),
        Some("signature-invalid"),
        "an edited receipt must not verify: {}",
        problem.message()
    );
}

#[test]
fn an_unsigned_receipt_still_matches_its_report() {
    // The property that needs no key, and the one most consumers actually want: the
    // report shown is the report the receipt describes.
    let report = a_report();
    let signed = crate::SignedReceipt {
        algorithm: crate::ALGORITHM.to_owned(),
        public_key: String::new(),
        signature: String::new(),
        receipt: crate::Receipt::issue(&report, "2026-01-15T12:00:00Z").unwrap(),
    };
    let verification = crate::verify(&signed, &report, None).unwrap();
    assert_eq!(verification.report_digest, signed.receipt.report_digest);
    assert!(!verification.is_attributed());
}

#[test]
fn the_crate_level_digest_helper_is_reachable() {
    report_digest_is_stable().unwrap();
}
