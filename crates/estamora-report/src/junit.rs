//! Rendering a report as `JUnit` XML.
//!
//! CI systems have parsed `JUnit` XML for two decades, and a conformance runner that
//! asks a project to write a bespoke parser will be integrated by fewer of them. So
//! the same results are emitted in the shape those systems already read, with one
//! deliberate difference from a unit-test report: a vector whose result is **not
//! decidable** is reported as an `error`, not as a `failure`, and a profile defect is
//! reported as an error too.
//!
//! That distinction is the whole point. A CI gate that treated an unreachable network
//! or a malformed profile as a failing test would blame the contract for the runner's
//! own circumstances, and the taxonomy Estamora publishes would be invisible exactly
//! where it matters most.
//!
//! # Escaping
//!
//! Every interpolated value is XML-escaped, and contract-supplied values additionally
//! cannot contain a valid XML character sequence by the time they arrive, because they
//! pass through the same sanitising the Markdown rendering uses. A test in this module
//! feeds hostile input through the whole path rather than asserting on the escaper
//! alone.

use estamora_core::Result;

use crate::markdown;
use crate::model::{Report, VectorResult};
use crate::render::{push, xml};

/// Renders a report as a `JUnit` `testsuites` document.
///
/// # Errors
///
/// Returns a report error if the document cannot be written, which would be a defect
/// of the renderer rather than of the input.
pub fn render(report: &Report) -> Result<String> {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");

    let totals = totals(&report.results);
    push(
        &mut out,
        format_args!(
            "<testsuites name=\"estamora\" tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\" time=\"0\">\n",
            totals.total, totals.failed, totals.errored, totals.skipped
        ),
    );

    let suite = format!(
        "{}@{}",
        markdown::sanitise(&report.profile.id),
        markdown::sanitise(&report.profile.version)
    );
    push(
        &mut out,
        format_args!(
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\" timestamp=\"{}\" hostname=\"{}\">\n",
            xml(&suite),
            totals.total,
            totals.failed,
            totals.errored,
            totals.skipped,
            xml(&markdown::sanitise(&report.generated_at)),
            xml(&markdown::sanitise(&report.target.network))
        ),
    );

    for result in &report.results {
        out.push_str(&one_case(result));
    }

    out.push_str("  </testsuite>\n");
    out.push_str("</testsuites>\n");
    Ok(out)
}

/// The counts a `JUnit` consumer reads off the envelope.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Totals {
    total: u32,
    failed: u32,
    errored: u32,
    skipped: u32,
}

impl Totals {
    /// Counts a checked sum, saturating rather than wrapping.
    fn add(&mut self, value: u32) {
        self.total = self.total.saturating_add(value);
    }
}

/// Counts the results.
fn totals(results: &[VectorResult]) -> Totals {
    let mut totals = Totals::default();
    for result in results {
        match result.status.as_str() {
            "passed" => totals.add(1),
            "failed" => {
                totals.add(1);
                totals.failed += 1;
            },
            "error" => {
                totals.add(1);
                // An undecidable vector is an environment-side problem to a JUnit
                // consumer, not a violated requirement.
                totals.errored += 1;
            },
            _ => {
                totals.add(1);
                totals.skipped += 1;
            },
        }
    }
    totals
}

/// One `<testcase>`, with its failure, error or skip child.
fn one_case(result: &VectorResult) -> String {
    let mut out = String::new();
    push(
        &mut out,
        format_args!(
            "    <testcase name=\"{}\" classname=\"{}\">\n",
            xml(&markdown::sanitise(&result.vector_id)),
            xml(&markdown::sanitise(&result.category))
        ),
    );

    match result.status.as_str() {
        "passed" => {},
        "error" => {
            let message = result
                .diagnostics
                .first()
                .map_or("the vector could not be decided", |finding| {
                    finding.message.as_str()
                });
            push(
                &mut out,
                format_args!(
                    "      <error message=\"{}\" type=\"UNDECIDABLE\">{}</error>\n",
                    xml(&markdown::sanitise(message)),
                    xml(&render_detail(result))
                ),
            );
        },
        "skipped" => {
            out.push_str("      <skipped/>\n");
        },
        _ => {
            let failures: Vec<String> = result
                .assertions
                .iter()
                .filter(|assertion| assertion.status == "failed")
                .map(|assertion| {
                    format!(
                        "{} [{}]: expected {}, observed {}",
                        assertion.id, assertion.category, assertion.expected, assertion.observed
                    )
                })
                .collect();
            let message = if failures.is_empty() {
                "the vector did not satisfy its requirements".to_owned()
            } else {
                failures.join("; ")
            };
            push(
                &mut out,
                format_args!(
                    "      <failure message=\"{}\" type=\"{}\">{}</failure>\n",
                    xml(&markdown::sanitise(&message)),
                    xml(&markdown::sanitise(&result.status)),
                    xml(&render_detail(result))
                ),
            );
        },
    }

    out.push_str("    </testcase>\n");
    out
}

/// The body of a failure or error element: every failing check, one per line.
fn render_detail(result: &VectorResult) -> String {
    let mut lines: Vec<String> = result
        .assertions
        .iter()
        .filter(|assertion| assertion.status == "failed")
        .map(|assertion| {
            format!(
                "{} [{}]\n  expected: {}\n  observed: {}",
                assertion.id,
                assertion.category,
                markdown::sanitise(&assertion.expected),
                markdown::sanitise(&assertion.observed)
            )
        })
        .collect();

    for finding in &result.diagnostics {
        lines.push(format!(
            "{}: {}",
            markdown::sanitise(&finding.code),
            markdown::sanitise(&finding.message)
        ));
    }

    if lines.is_empty() {
        lines.push("No individual check failed.".to_owned());
    }
    lines.join("\n")
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::render;
    use crate::model::{
        CorpusIdentity, ProfileIdentity, Report, ReportedAssertion, ReportedDiagnostic,
        RunnerIdentity, Summary, Target, VectorResult,
    };
    use estamora_core::ConformanceStatus;
    use std::collections::BTreeMap;

    fn report_with(status: &str, assertions: Vec<ReportedAssertion>) -> Report {
        Report {
            schema: None,
            estamora_spec_version: "1.0".to_owned(),
            runner: RunnerIdentity {
                name: "estamora".to_owned(),
                version: "0.1.0".to_owned(),
            },
            generated_at: "2026-01-15T12:00:00Z".to_owned(),
            target: Target {
                contract: "CABC".to_owned(),
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
                count: 1,
            },
            configuration: BTreeMap::new(),
            results: vec![VectorResult {
                vector_id: "transfer-moves-exact-amount".to_owned(),
                category: "positive".to_owned(),
                status: status.to_owned(),
                assertions,
                diagnostics: Vec::new(),
            }],
            summary: Summary::default(),
            status: ConformanceStatus::NonConformant,
            exit_code: 1,
        }
    }

    #[test]
    fn an_undecidable_vector_is_an_error_rather_than_a_failure() {
        // A CI gate that read a network outage as a violated requirement would
        // blame the contract for the runner's circumstances, and the taxonomy the
        // runner publishes would be invisible exactly where it matters.
        let mut report = report_with("error", Vec::new());
        report.results[0].diagnostics.push(ReportedDiagnostic {
            code: "interface-unavailable".to_owned(),
            message: "the artifact could not be read".to_owned(),
        });
        let xml = render(&report).unwrap();
        assert!(xml.contains("<error "), "{xml}");
        assert!(!xml.contains("<failure "), "{xml}");
        assert!(xml.contains("type=\"UNDECIDABLE\""), "{xml}");
    }

    #[test]
    fn a_violated_vector_is_a_failure() {
        let report = report_with(
            "failed",
            vec![ReportedAssertion {
                id: "state-sender-debited".to_owned(),
                category: "state".to_owned(),
                status: "failed".to_owned(),
                expected: "750".to_owned(),
                observed: "500".to_owned(),
                detail: None,
            }],
        );
        let xml = render(&report).unwrap();
        assert!(xml.contains("<failure "), "{xml}");
        assert!(xml.contains("750"), "{xml}");
    }

    #[test]
    fn hostile_content_cannot_close_an_attribute() {
        let mut report = report_with("failed", Vec::new());
        report.results[0].diagnostics.push(ReportedDiagnostic {
            code: "x".to_owned(),
            message: "\"/><testsuites failures=\"0".to_owned(),
        });
        let xml = render(&report).unwrap();
        assert!(!xml.contains("/><testsuites"), "{xml}");
        assert!(xml.contains("&quot;"), "{xml}");
    }

    #[test]
    fn the_envelope_counts_what_its_children_carry() {
        let report = report_with("error", Vec::new());
        let xml = render(&report).unwrap();
        assert!(
            xml.contains("tests=\"1\" failures=\"0\" errors=\"1\" skipped=\"0\""),
            "{xml}"
        );
    }
}
