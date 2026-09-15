//! Rendering for a person.
//!
//! Every function here returns a string. Nothing in this crate writes to standard
//! output, which is a deliberate constraint rather than a style: the workspace forbids
//! `print_stdout`, so the rendering is pure and a test can pin what a user sees without
//! capturing a process's output — and the JSON document, which is the artefact another
//! tool reads, can never be contaminated by a line intended for a terminal.
//!
//! # What a terminal rendering always says
//!
//! The last block every rendering ends with is what the result does **not** establish.
//! A pass rate is the most quotable thing a conformance run produces and the most
//! easily read as a guarantee, so the sentence that prevents that reading travels with
//! the number rather than living only in a document nobody opens.

use std::fmt::Write as _;
use std::path::Path;

use estamora_core::{AssertionStatus, ConformanceStatus, Diagnostic, Severity, VectorStatus};
use estamora_report::AssertionSummary;
use estamora_report::model::Report;
use estamora_report::model::category_of;

use crate::engine::{Inspection, Validated};

/// The tick and cross a check is rendered with.
const PASSED: &str = "✓";
const FAILED: &str = "✗";

/// The sentence every rendering ends with.
pub const LIMITATIONS: &str = "\
Conformance is not security. This result says the contract satisfied the named profile \
over the named corpus; it does not say the contract is free of vulnerabilities, and \
passing does not replace a formal verification, an audit or a penetration test. A \
profile is a document with an author: which profile, at which revision, and from where, \
are part of what this result means.";

/// A run's terminal rendering.
///
/// `verbose` lists every check performed, grouped by the dimension it belongs to. The
/// default lists only what did not hold, because a reader scanning a run wants the
/// defects; the dimensions are always summarised either way, so a passing interface
/// section can never be mistaken for evidence about behaviour.
#[must_use]
pub fn report(report: &Report, verbose: bool) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", rule());
    let _ = writeln!(
        out,
        "Estamora {} · {}@{} · {}",
        report.runner.version, report.profile.id, report.profile.version, report.target.network
    );
    let _ = writeln!(out, "contract  {}", report.target.contract);
    let _ = writeln!(
        out,
        "artifact  {}",
        report
            .target
            .wasm_hash
            .as_deref()
            .unwrap_or("not available")
    );
    let _ = writeln!(
        out,
        "profile   {} · vectors {}",
        report.profile.digest, report.vectors.count
    );
    let _ = writeln!(out, "{}", rule());

    for result in &report.results {
        let marker = match result.status.as_str() {
            "passed" => PASSED,
            "failed" => FAILED,
            _ => "•",
        };
        let _ = writeln!(out, "{marker} {} ({})", result.vector_id, result.status);
        for assertion in &result.assertions {
            if !verbose && assertion.status != "failed" {
                continue;
            }
            let dimension = category_of(&assertion.category).map_or_else(
                || assertion.category.clone(),
                |category| category.as_str().to_owned(),
            );
            let _ = writeln!(
                out,
                "    [{dimension}] {} {}",
                if assertion.status == "failed" {
                    FAILED
                } else {
                    PASSED
                },
                assertion.detail.as_deref().unwrap_or(&assertion.id)
            );
            if assertion.status == "failed" {
                let _ = writeln!(out, "        expected: {}", assertion.expected);
                let _ = writeln!(out, "        observed: {}", assertion.observed);
            }
        }
        for finding in &result.diagnostics {
            let _ = writeln!(out, "    · {} {}", finding.code, finding.message);
        }
    }

    let _ = writeln!(out, "{}", rule());
    let _ = writeln!(out, "checks by dimension");
    for (category, tally) in report.summary.all() {
        let _ = writeln!(
            out,
            "  {:<14} {}",
            category.as_str(),
            describe_summary(&tally)
        );
    }
    let _ = writeln!(
        out,
        "  {:<14} {} reported, {} failed",
        "total",
        report.summary.total(),
        report.summary.failed()
    );

    let _ = writeln!(out, "{}", rule());
    let _ = writeln!(out, "status {} (exit {})", report.status, report.exit_code);
    let _ = writeln!(out, "{}", rule());
    let _ = writeln!(out, "{LIMITATIONS}");
    out
}

/// A dimension's tally, in words where words are clearer than numbers.
fn describe_summary(tally: &AssertionSummary) -> String {
    if tally.total == 0 {
        return "not checked".to_owned();
    }
    let mut rendered = format!("{} passed, {} failed", tally.passed, tally.failed);
    if tally.warnings > 0 {
        let _ = write!(rendered, ", {} warning(s)", tally.warnings);
    }
    rendered
}

/// A profile and corpus validation, rendered for a terminal.
#[must_use]
pub fn validated(validated: &Validated) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", rule());
    let _ = writeln!(out, "profile  {}", validated.reference);
    let _ = writeln!(out, "format   estamora-spec {}", validated.spec_version);
    let _ = writeln!(out, "digest   {}", validated.profile_digest);
    let _ = writeln!(
        out,
        "corpus   {} · {}",
        validated.corpus_digest,
        validated.vectors.len()
    );
    let _ = writeln!(out, "{}", rule());
    let _ = writeln!(out, "requirements declared");
    for (what, count) in &validated.requirements {
        let _ = writeln!(out, "  {what:<22} {count}");
    }
    let _ = writeln!(out, "vectors");
    for id in &validated.vectors {
        let _ = writeln!(out, "  {id}");
    }
    let _ = writeln!(out, "{}", rule());
    let _ = write!(out, "{}", findings(&validated.findings));
    out
}

/// An interface inspection, rendered for a terminal.
#[must_use]
pub fn inspection(inspection: &Inspection) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", rule());
    let _ = writeln!(out, "contract  {}", inspection.target);
    let _ = writeln!(out, "network   {}", inspection.network);
    let _ = writeln!(
        out,
        "artifact  {}",
        inspection.wasm_hash.as_deref().unwrap_or("not available")
    );
    let _ = writeln!(out, "seeding   {}", inspection.seeding.description());
    let _ = writeln!(out, "source    {}", inspection.interface.source);
    let _ = writeln!(out, "{}", rule());
    if inspection.interface.methods.is_empty() {
        let _ = writeln!(out, "the contract publishes no methods");
    }
    for method in &inspection.interface.methods {
        let parameters: Vec<&str> = method
            .parameters
            .iter()
            .map(|parameter| parameter.type_name.as_str())
            .collect();
        let _ = writeln!(
            out,
            "{PASSED} {}({}) -> {}{}",
            method.name,
            parameters.join(", "),
            method.returns.as_deref().unwrap_or("void"),
            if method.readonly { "  [read-only]" } else { "" }
        );
    }
    let _ = writeln!(out, "{}", rule());
    let _ = writeln!(
        out,
        "An interface is the weakest claim Estamora makes. A contract that exposes every \
         method a profile declares can still violate every behavioural requirement in it, \
         which is why a run measured against a profile is the only thing that supports a \
         conformance verdict."
    );
    out
}

/// Findings that did not stop a run.
#[must_use]
pub fn findings(findings: &[Diagnostic]) -> String {
    let mut out = String::new();
    if findings.is_empty() {
        let _ = writeln!(out, "no findings");
        return out;
    }
    for finding in findings {
        let level = match finding.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let _ = writeln!(out, "{level}: {} {}", finding.class, finding.message);
        for (key, value) in &finding.context {
            let _ = writeln!(out, "  {key}: {value}");
        }
    }
    out
}

/// A one-line summary of a run's verdict, for a caller that wants nothing else.
#[must_use]
pub fn verdict(status: ConformanceStatus, exit_code: i32) -> String {
    format!("{status} (exit {exit_code})")
}

/// A vector's status, as a terminal marks it.
#[must_use]
pub const fn marker(status: VectorStatus) -> &'static str {
    match status {
        VectorStatus::Passed => PASSED,
        VectorStatus::Failed => FAILED,
        VectorStatus::Error | VectorStatus::Skipped => "•",
    }
}

/// An assertion's status, as a terminal marks it.
#[must_use]
pub const fn assertion_marker(status: AssertionStatus) -> &'static str {
    match status {
        AssertionStatus::Passed => PASSED,
        AssertionStatus::Failed => FAILED,
    }
}

/// Writes `contents` to `path`.
///
/// # Errors
///
/// Returns a report error when the file cannot be written. A report that could not be
/// written is reported as a failure of the report rather than of the run: the verdict
/// was reached, and losing it to a filesystem problem must not be confused with
/// reaching a different one.
pub fn write(path: &Path, contents: &str) -> estamora_core::Result<()> {
    // A path with no directory component has nothing to create, and a directory that
    // already exists is left alone: the filter says which parents are work rather than
    // nesting a second condition inside the first.
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty() && !parent.is_dir())
    {
        std::fs::create_dir_all(parent).map_err(|problem| {
            estamora_core::Error::new(
                estamora_core::ErrorClass::ReportError,
                format!(
                    "the directory for {} could not be created: {problem}",
                    path.display()
                ),
            )
        })?;
    }
    std::fs::write(path, contents).map_err(|problem| {
        estamora_core::Error::new(
            estamora_core::ErrorClass::ReportError,
            format!(
                "the report at {} could not be written: {problem}",
                path.display()
            ),
        )
        .with_context("path", path.display().to_string())
    })
}

/// A horizontal rule.
fn rule() -> String {
    "─".repeat(72)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{LIMITATIONS, findings, write};
    use estamora_core::{Diagnostic, ErrorClass};

    #[test]
    fn the_limitation_is_stated_once_and_says_what_a_pass_does_not_mean() {
        assert!(LIMITATIONS.contains("not security"));
        assert!(LIMITATIONS.contains("passing does not replace"));
        // Provenance is what a result means, so all three facts must be named.
        assert!(LIMITATIONS.contains("which profile"));
        assert!(LIMITATIONS.contains("which revision"));
        assert!(LIMITATIONS.contains("from where"));
    }

    #[test]
    fn a_finding_carries_its_context_so_a_reader_can_act_on_it() {
        let finding = Diagnostic::error(ErrorClass::VectorError, "the vector is unusable")
            .with_context("vector", "transfer-moves-exact-amount");
        let rendered = findings(&[finding]);
        assert!(rendered.contains("VECTOR_ERROR"));
        assert!(rendered.contains("vector: transfer-moves-exact-amount"));
    }

    #[test]
    fn writing_a_report_creates_its_directory_and_survives_being_asked_to_write_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/report.json");
        write(&path, "{}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
    }
}
