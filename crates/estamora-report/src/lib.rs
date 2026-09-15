//! Rendering a conformance run.
//!
//! A run produces results; a report is what someone other than the run reads. Three
//! renderings are provided and only one of them is normative:
//!
//! | Rendering | Consumer | Normative |
//! | --------- | -------- | --------- |
//! | [`json`] | Other tools, receipts | Yes — it mirrors the specification's report schema |
//! | [`markdown`] | A reviewer reading a pull request | No |
//! | [`junit`] | An existing CI gate | No |
//!
//! The Markdown and `JUnit` renderings must never disagree with the JSON document. They
//! are derived from the same [`model::Report`], so a disagreement would be a defect in
//! a renderer rather than a difference of opinion between two sources of truth.
//!
//! # What every rendering has to say
//!
//! Each one states what the result does **not** establish. Conformance is not
//! security, passing vectors do not prove the absence of vulnerabilities, and a
//! profile is a document with an author whose provenance and version matter. A report
//! that omitted that would invite its reader to read a pass rate as a guarantee, which
//! is the reading this project exists to prevent.
#![forbid(unsafe_code)]

pub mod identifier;
pub mod json;
pub mod junit;
pub mod markdown;
pub mod model;
pub mod render;

pub use identifier::{IDENTIFIER_LIMIT, identifier};
pub use json::{parse, render as render_json};
pub use junit::render as render_junit;
pub use markdown::render as render_markdown;
pub use model::{
    AssertionSummary, CorpusIdentity, ProfileIdentity, REPORT_SCHEMA, Report, ReportedAssertion,
    ReportedDiagnostic, RunnerIdentity, Summary, Target, VectorResult, bounded, report_assertion,
    report_vector, tally,
};

#[cfg(test)]
mod tests;
