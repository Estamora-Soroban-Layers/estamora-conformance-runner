//! The conformance report, as the specification's report schema defines it.
//!
//! The specification repository owns the semantics of this document even though the
//! runner produces it, so this model mirrors `schema/report.schema.json` field for
//! field rather than the schema being derived from the model. An independent
//! implementation in another language must be able to emit a document Estamora
//! tooling accepts, and a runner whose internal model differed from the published
//! one would translate between them — and would eventually translate something
//! wrong.
//!
//! # Two properties that shape every field here
//!
//! **Results are per assertion and per dimension.** Interface compatibility,
//! authorization, events, behaviour, state, invariants and failure are independent
//! claims about a contract. Collapsing them makes a caller unable to see which claim
//! failed, and lets a passing interface section be presented as evidence of correct
//! behaviour.
//!
//! **The final status is one of six explicit values.** A non-conformant contract and
//! a broken execution environment must not be reportable the same way, and neither
//! may be read as a security guarantee.
//!
//! # Why assertions carry text rather than values
//!
//! Expected and observed are retained as rendered text, bounded by the schema's own
//! maxima. A report is read by someone who was not present at the run and often
//! cannot re-run it, because the target is a remote network — so every check has to
//! be diagnosable from the document itself.
//!
//! # Why the summary counts distinct checks
//!
//! Interface compatibility is a property of the contract rather than of a scenario,
//! so its checks are attached to every vector result. Counting raw occurrences would
//! report a larger body of evidence than exists; the summary therefore counts each
//! assertion identifier once.

use std::collections::{BTreeMap, BTreeSet};

use estamora_core::{ConformanceStatus, RunTally, VectorStatus};
use estamora_vectors::AssertionCategory;
use serde::{Deserialize, Serialize};

use crate::identifier::identifier;

/// The schema a report declares itself written against.
pub const REPORT_SCHEMA: &str = "https://estamora.dev/schema/report.schema.json";

/// The longest vector identifier the schema permits.
const VECTOR_ID_LIMIT: usize = 96;

/// The shortest vector identifier the schema permits.
const VECTOR_ID_MINIMUM: usize = 4;

/// The longest rendered expectation or observation the schema permits.
const VALUE_LIMIT: usize = 2_000;

/// The longest explanation the schema permits.
const DETAIL_LIMIT: usize = 4_000;

/// A complete conformance report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// The schema this document is written against.
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// The Estamora specification format version the report is written against.
    pub estamora_spec_version: String,
    /// Which implementation produced it, and at which version.
    ///
    /// Required for reproducibility: two runners may differ in behaviour, and a
    /// report that does not name its producer cannot be re-verified.
    pub runner: RunnerIdentity,
    /// When the report was produced, as an RFC 3339 date-time.
    pub generated_at: String,
    /// What was measured.
    pub target: Target,
    /// Which requirement set the result is attributed to, pinned by digest so that
    /// a result cannot be silently re-interpreted against a later revision of the
    /// same profile version.
    pub profile: ProfileIdentity,
    /// The corpus that was executed, pinned by digest.
    pub vectors: CorpusIdentity,
    /// Execution configuration that could change the result.
    ///
    /// Recorded so that an unexplained difference between two runs of the same
    /// vectors against the same contract can be diagnosed rather than guessed at.
    pub configuration: BTreeMap<String, serde_json::Value>,
    /// One entry per executed vector.
    pub results: Vec<VectorResult>,
    /// Per-dimension tallies.
    pub summary: Summary,
    /// The run's verdict.
    pub status: ConformanceStatus,
    /// The process exit code the runner used, so that CI can reproduce the
    /// classification without re-deriving it.
    pub exit_code: i32,
}

/// The producer's identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerIdentity {
    /// The tool's name.
    pub name: String,
    /// Its version, independent of the profile version and of the format version.
    pub version: String,
}

/// What was measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    /// The contract identifier.
    pub contract: String,
    /// The network, e.g. `local`, `testnet`, `mainnet`.
    pub network: String,
    /// The contract's WebAssembly hash, where it could be resolved.
    ///
    /// Reported as `null` rather than omitted when it could not be, so that the
    /// absence is visible to a reader rather than indistinguishable from a field the
    /// producer did not implement.
    pub wasm_hash: Option<String>,
    /// Contract-supplied metadata, treated as untrusted input.
    ///
    /// Rendered reports sanitise it, because this content is attacker-controlled.
    pub metadata: BTreeMap<String, serde_json::Value>,
}

/// The profile a result is attributed to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileIdentity {
    /// The profile's identifier.
    pub id: String,
    /// Its revision.
    pub version: String,
    /// A content digest of the bundle, as `sha256:<hex>`.
    pub digest: String,
}

/// The corpus a result was produced from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusIdentity {
    /// A content digest of the executed vectors, as `sha256:<hex>`.
    pub digest: String,
    /// How many vectors were executed.
    pub count: u32,
}

/// Per-dimension tallies.
///
/// Dimensions are kept separate so that a passing interface section can never stand
/// in for behavioural conformance.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    /// Interface compatibility checks.
    pub interface: AssertionSummary,
    /// Authorization checks.
    pub authorization: AssertionSummary,
    /// Event checks.
    pub events: AssertionSummary,
    /// Behavioural checks.
    pub behavior: AssertionSummary,
    /// State checks.
    pub state: AssertionSummary,
    /// Invariant checks.
    pub invariants: AssertionSummary,
    /// Failure checks.
    pub failure: AssertionSummary,
}

/// A tally for one dimension.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssertionSummary {
    /// Checks that held.
    pub passed: u32,
    /// Checks that did not hold.
    pub failed: u32,
    /// Checks reported in total.
    pub total: u32,
    /// Violated warnings, recorded and excluded from the verdict.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub warnings: u32,
}

/// Whether a count is zero, for omitting an optional field.
///
/// Taken by reference because that is the signature `serde` requires of a
/// `skip_serializing_if` predicate.
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if predicate is passed the field by reference"
)]
fn is_zero(count: &u32) -> bool {
    *count == 0
}

impl Summary {
    /// Tallies every result, counting each assertion identifier once.
    ///
    /// The deduplication is not cosmetic. Interface checks are a property of the
    /// contract rather than of a scenario, so the same assertion identifiers appear
    /// in every vector's result; counting raw occurrences would multiply the
    /// evidence by the size of the corpus.
    ///
    /// An identifier that failed in any vector is counted as failed, however many
    /// other vectors it held in. Reporting the first occurrence instead would let a
    /// requirement that one scenario violated be summarised as satisfied because a
    /// different scenario had satisfied it — which is the summary telling a reader
    /// the opposite of what the run found.
    #[must_use]
    pub fn of(results: &[VectorResult]) -> Self {
        let mut summary = Self::default();
        // Identifier to whether it failed anywhere, so that the worst result each
        // identifier reached is the one counted.
        let mut seen: BTreeMap<&str, bool> = BTreeMap::new();
        for result in results {
            for assertion in &result.assertions {
                let failed = assertion.status == "failed";
                seen.entry(assertion.id.as_str())
                    .and_modify(|already| *already |= failed)
                    .or_insert(failed);
            }
            // A violated warning-severity invariant is recorded as a diagnostic
            // rather than as a failed check, so that an upstream SHOULD cannot turn
            // a passing run into a failing one. It is counted here so that the
            // report still makes the finding visible.
            let warnings = result
                .diagnostics
                .iter()
                .filter(|finding| finding.code.starts_with("invariant-warning"))
                .count();
            summary.invariants.warnings = summary
                .invariants
                .warnings
                .saturating_add(u32::try_from(warnings).unwrap_or(u32::MAX));
        }

        // One pass over the deduplicated identifiers, so that the category each
        // belongs to is read from the result it appeared in rather than from a
        // second traversal of every vector.
        let categories: BTreeMap<&str, &str> = results
            .iter()
            .flat_map(|result| result.assertions.iter())
            .map(|assertion| (assertion.id.as_str(), assertion.category.as_str()))
            .collect();
        for (id, failed) in seen {
            let Some(category) = categories.get(id).and_then(|name| category_of(name)) else {
                // A category outside the seven cannot be produced by this writer,
                // and the report schema rejects it, so a document carrying one is
                // not one this runner made. Counting it would invent a dimension.
                continue;
            };
            let tally = summary.dimension(category);
            tally.total += 1;
            if failed {
                tally.failed += 1;
            } else {
                tally.passed += 1;
            }
        }
        summary
    }

    /// The tally belonging to a dimension.
    fn dimension(&mut self, category: AssertionCategory) -> &mut AssertionSummary {
        match category {
            AssertionCategory::Interface => &mut self.interface,
            AssertionCategory::Authorization => &mut self.authorization,
            AssertionCategory::Event => &mut self.events,
            AssertionCategory::Behavior => &mut self.behavior,
            AssertionCategory::State => &mut self.state,
            AssertionCategory::Invariant => &mut self.invariants,
            AssertionCategory::Failure => &mut self.failure,
        }
    }

    /// Every dimension's tally, in the order the schema lists them.
    #[must_use]
    pub fn all(&self) -> [(AssertionCategory, AssertionSummary); 7] {
        [
            (AssertionCategory::Interface, self.interface),
            (AssertionCategory::Authorization, self.authorization),
            (AssertionCategory::Event, self.events),
            (AssertionCategory::Behavior, self.behavior),
            (AssertionCategory::State, self.state),
            (AssertionCategory::Invariant, self.invariants),
            (AssertionCategory::Failure, self.failure),
        ]
    }

    /// Checks that were reported, summed over every dimension.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.all()
            .iter()
            .map(|(_, tally)| tally.total)
            .fold(0, u32::saturating_add)
    }

    /// Checks that did not hold, summed over every dimension.
    #[must_use]
    pub fn failed(&self) -> u32 {
        self.all()
            .iter()
            .map(|(_, tally)| tally.failed)
            .fold(0, u32::saturating_add)
    }
}

/// The result of one executed vector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorResult {
    /// The vector's identifier, normalised to the schema's identifier grammar.
    pub vector_id: String,
    /// The class of property the vector probes.
    pub category: String,
    /// Whether the vector passed, failed, errored or was skipped.
    pub status: String,
    /// Every check that was reported.
    pub assertions: Vec<ReportedAssertion>,
    /// Findings that are not checks.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub diagnostics: Vec<ReportedDiagnostic>,
}

/// One reported check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportedAssertion {
    /// A stable identifier, normalised to the schema's identifier grammar.
    ///
    /// The assertion layer's identifiers are structured and sometimes longer than
    /// the grammar permits, so they are normalised here and the original is kept in
    /// `detail`. Losing the structure would make a failure harder to locate; losing
    /// the grammar would make the document invalid, and an invalid report is one no
    /// independent tool can read.
    pub id: String,
    /// Which conformance dimension the check belongs to.
    pub category: String,
    /// Whether the check held.
    pub status: String,
    /// What the requirement asked for.
    pub expected: String,
    /// What was observed.
    pub observed: String,
    /// Anything further a reader needs.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub detail: Option<String>,
}

/// A finding that is not a check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportedDiagnostic {
    /// A stable short code.
    pub code: String,
    /// What was found.
    pub message: String,
}

/// Converts an assertion outcome into its reported form.
#[must_use]
pub fn report_assertion(outcome: &estamora_assertions::AssertionOutcome) -> ReportedAssertion {
    let normalised = identifier(&outcome.id);
    let mut detail = format!("check: {}", outcome.id);
    if let Some(extra) = &outcome.detail {
        detail.push('\n');
        detail.push_str(extra);
    }
    ReportedAssertion {
        id: normalised,
        category: outcome.category.as_str().to_owned(),
        status: outcome.status.as_str().to_owned(),
        expected: bounded(&outcome.expected, VALUE_LIMIT),
        observed: bounded(&outcome.observed, VALUE_LIMIT),
        detail: Some(bounded(&detail, DETAIL_LIMIT)),
    }
}

/// Converts a vector outcome into its reported form.
#[must_use]
pub fn report_vector(outcome: &estamora_assertions::VectorOutcome) -> VectorResult {
    VectorResult {
        vector_id: bounded(&outcome.vector_id, VECTOR_ID_LIMIT),
        category: bounded(&outcome.category, 32),
        status: outcome.status.as_str().to_owned(),
        assertions: outcome.assertions.iter().map(report_assertion).collect(),
        diagnostics: outcome
            .diagnostics
            .iter()
            .map(|finding| ReportedDiagnostic {
                code: bounded(&identifier(&finding.code), 64),
                message: bounded(&finding.message, VALUE_LIMIT),
            })
            .collect(),
    }
}

/// Reduces a run's per-vector statuses to the six-value verdict.
///
/// The rule lives in `estamora-core` and is applied there; this function exists so
/// that the report and the process exit code cannot be derived from two different
/// reductions of the same results.
#[must_use]
pub fn tally(results: &[VectorResult], required: &BTreeSet<String>) -> RunTally {
    let mut tally = RunTally::default();
    for result in results {
        let status = match result.status.as_str() {
            "passed" => VectorStatus::Passed,
            "failed" => VectorStatus::Failed,
            "error" => VectorStatus::Error,
            _ => VectorStatus::Skipped,
        };
        tally.record(status, required.contains(&result.vector_id));
    }
    tally
}

/// Truncates a string to the schema's maximum, marking the truncation.
///
/// The marker matters: a value silently cut short would read as a complete
/// observation, and an observation that was actually shorter than it looks is worse
/// than one that is visibly incomplete.
#[must_use]
pub fn bounded(text: &str, limit: usize) -> String {
    const MARKER: &str = "…";
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let keep = limit.saturating_sub(MARKER.chars().count());
    let mut truncated: String = text.chars().take(keep).collect();
    truncated.push_str(MARKER);
    truncated
}

/// The dimension a reported category names, where it names one.
///
/// Exposed so that a caller can group a report's checks without matching on a
/// string, which is the kind of implicit convention that breaks silently the first
/// time a spelling changes.
#[must_use]
pub fn category_of(name: &str) -> Option<AssertionCategory> {
    AssertionCategory::ALL
        .into_iter()
        .find(|category| category.as_str() == name)
}

/// Whether a vector identifier is one the schema accepts.
#[must_use]
pub fn vector_id_is_acceptable(id: &str) -> bool {
    (VECTOR_ID_MINIMUM..=VECTOR_ID_LIMIT).contains(&id.chars().count())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{Summary, VectorResult, bounded};

    #[test]
    fn truncation_is_visible_rather_than_silent() {
        let long = "a".repeat(2_100);
        let cut = bounded(&long, 2_000);
        assert_eq!(cut.chars().count(), 2_000);
        assert!(cut.ends_with('…'), "a truncated value must say so");
        assert_eq!(bounded("short", 2_000), "short");
    }

    #[test]
    fn a_repeated_interface_check_is_counted_once() {
        // Every vector result carries the interface checks, because interface
        // compatibility is a property of the contract rather than of a scenario.
        // Counting occurrences would multiply the evidence by the corpus size.
        let assertion = super::ReportedAssertion {
            id: "interface-method-transfer".to_owned(),
            category: "interface".to_owned(),
            status: "passed".to_owned(),
            expected: "present".to_owned(),
            observed: "present".to_owned(),
            detail: None,
        };
        let results = vec![
            VectorResult {
                vector_id: "one".to_owned(),
                category: "positive".to_owned(),
                status: "passed".to_owned(),
                assertions: vec![assertion.clone()],
                diagnostics: Vec::new(),
            },
            VectorResult {
                vector_id: "two".to_owned(),
                category: "positive".to_owned(),
                status: "passed".to_owned(),
                assertions: vec![assertion],
                diagnostics: Vec::new(),
            },
        ];
        let summary = Summary::of(&results);
        assert_eq!(summary.interface.total, 1);
        assert_eq!(summary.total(), 1);
    }
}
