//! The canonical rendering.
//!
//! JSON is the report. The Markdown and `JUnit` renderings exist for people and for
//! CI systems, and neither is what another tool consumes — so this module is where
//! the document's identity is decided, and the other two are conveniences that must
//! never disagree with it.
//!
//! # Round-tripping is deliberate
//!
//! A report deserialises back into the model. That is what makes a receipt
//! verifiable: a receipt commits to the digest of a document, and verification has
//! to reconstruct the document to check the commitment. It is also how the schema
//! conformance tests read a produced report rather than comparing strings.

use estamora_core::{Error, ErrorClass, Result};

use crate::model::Report;

/// Renders a report as JSON.
///
/// `pretty` produces the shape a person reads; the compact form is what a receipt
/// digests, because a digest over whitespace would change if a formatter's policy
/// did — and two runs that produced the same result would stop verifying.
///
/// # Errors
///
/// Returns a report error when the document cannot be serialised, which would mean
/// the model held a value JSON cannot represent.
pub fn render(report: &Report, pretty: bool) -> Result<String> {
    let rendered = if pretty {
        serde_json::to_string_pretty(report)
    } else {
        serde_json::to_string(report)
    };
    rendered.map_err(|problem| {
        Error::new(
            ErrorClass::ReportError,
            format!("the report could not be serialised: {problem}"),
        )
    })
}

/// Reads a report back.
///
/// # Errors
///
/// Returns a report error when the document is not a report this runner can
/// understand. An unreadable report is never reported as a failed run: the run's
/// result is unknown, so no verdict about a contract follows from it.
pub fn parse(text: &str) -> Result<Report> {
    serde_json::from_str(text).map_err(|problem| {
        Error::new(
            ErrorClass::ReportError,
            format!("the document is not a conformance report: {problem}"),
        )
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{parse, render};
    use crate::model::{CorpusIdentity, ProfileIdentity, Report, RunnerIdentity, Summary, Target};
    use estamora_core::ConformanceStatus;
    use std::collections::BTreeMap;

    fn a_report() -> Report {
        Report {
            schema: Some(crate::model::REPORT_SCHEMA.to_owned()),
            estamora_spec_version: "1.0".to_owned(),
            runner: RunnerIdentity {
                name: "estamora".to_owned(),
                version: "0.1.0".to_owned(),
            },
            generated_at: "2026-01-15T12:00:00Z".to_owned(),
            target: Target {
                contract: "CABCDEF".to_owned(),
                network: "local".to_owned(),
                wasm_hash: None,
                metadata: BTreeMap::new(),
            },
            profile: ProfileIdentity {
                id: "sep-41".to_owned(),
                version: "1.0".to_owned(),
                digest: format!("sha256:{}", "0".repeat(64)),
            },
            vectors: CorpusIdentity {
                digest: format!("sha256:{}", "1".repeat(64)),
                count: 0,
            },
            configuration: BTreeMap::new(),
            results: Vec::new(),
            summary: Summary::default(),
            status: ConformanceStatus::Inconclusive,
            exit_code: 2,
        }
    }

    #[test]
    fn a_report_survives_a_round_trip() {
        let report = a_report();
        let text = render(&report, true).unwrap();
        assert_eq!(parse(&text).unwrap(), report);
    }

    #[test]
    fn the_compact_form_carries_no_incidental_whitespace() {
        // What a receipt digests. Two runs that reached the same result must
        // produce the same bytes, so the digest cannot depend on a pretty-printer's
        // indentation policy.
        let report = a_report();
        let compact = render(&report, false).unwrap();
        assert!(!compact.contains('\n'));
        assert_eq!(compact, render(&report, false).unwrap());
    }

    #[test]
    fn a_document_that_is_not_a_report_is_refused() {
        let problem = parse("{\"hello\":\"world\"}").unwrap_err();
        assert_eq!(problem.class(), estamora_core::ErrorClass::ReportError);
    }
}
