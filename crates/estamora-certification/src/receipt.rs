//! The certification receipt.
//!
//! A receipt is a small, self-contained commitment to a run's result. It names what
//! was measured, against which pinned requirement set and corpus, what the verdict
//! was, and the digest of the report that carries the detail — so that a third party
//! holding only the receipt can tell whether a report shown to them is the one it is
//! about.
//!
//! # What a receipt is not
//!
//! It is not a statement that a contract is correct or safe. It is a statement that a
//! named runner reached a named verdict under a named profile against a named
//! contract, and it is verifiable to exactly that extent. Everything the report's
//! limitation section says applies here and more so, because a receipt is what gets
//! quoted out of context.
//!
//! # The runner does not implement an on-chain certification contract
//!
//! Estamora's specification layer defines the normative data model and the
//! semantics; the runner produces a receipt and can verify one. Publishing a receipt
//! to a ledger, if that ever happens, belongs elsewhere and is not implemented here.
//! Adding it would make the open-source runner depend on infrastructure the project
//! does not own, and would invite reading an on-chain record as a stronger claim than
//! this document supports.
//!
//! # Pinning
//!
//! The profile digest and the corpus digest are both recorded. A result is only
//! interpretable against the exact revision of the requirements that produced it, and
//! a profile version alone does not identify a revision: a behavioural requirement can
//! change while the version string stays the same, and a consumer comparing only
//! versions would compare two different standards.

use estamora_core::{ConformanceStatus, Error, ErrorClass, Result};
use estamora_report::model::{Report, RunnerIdentity, Summary};
use serde::{Deserialize, Serialize};

use crate::digest::Digest;

/// A commitment to one conformance result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    /// The Estamora specification format version this receipt is written against.
    pub estamora_spec_version: String,
    /// Which implementation reached the verdict, and at which version.
    pub runner: RunnerIdentity,
    /// When it was issued, as an RFC 3339 date-time.
    pub issued_at: String,
    /// The requirement set the verdict is attributed to, pinned by digest.
    pub profile: ReceiptProfile,
    /// The corpus that was executed, pinned by digest.
    pub vectors: ReceiptCorpus,
    /// What was measured.
    pub target: ReceiptTarget,
    /// The verdict.
    pub result: ReceiptResult,
    /// The digest of the report carrying the detail.
    pub report_digest: Digest,
}

/// The profile a receipt is attributed to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptProfile {
    /// The profile's identifier.
    pub id: String,
    /// Its revision.
    pub version: String,
    /// The content digest of the exact bundle revision used.
    pub digest: Digest,
}

/// The corpus a receipt is attributed to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptCorpus {
    /// The content digest of the executed vectors.
    pub digest: Digest,
    /// How many were executed.
    pub count: u32,
}

/// What was measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptTarget {
    /// The contract's identifier.
    pub contract: String,
    /// The network it was measured on.
    pub network: String,
    /// Its WebAssembly hash, where one could be resolved.
    ///
    /// `None` is recorded rather than omitted: a receipt that silently lacked the
    /// strongest available identity for a contract would be read as though the hash
    /// had matched.
    pub wasm_hash: Option<String>,
}

/// The verdict a receipt records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReceiptResult {
    /// One of the six statuses.
    pub status: ConformanceStatus,
    /// The process exit code, so that CI can reproduce the classification.
    pub exit_code: i32,
    /// The per-dimension tallies, so that the receipt is meaningful without the
    /// report it commits to.
    pub summary: Summary,
}

impl Receipt {
    /// Issues a receipt for a report.
    ///
    /// # Errors
    ///
    /// Returns a certification error if the report cannot be canonicalised, which
    /// would mean the report holds a value JSON cannot represent.
    pub fn issue(report: &Report, issued_at: impl Into<String>) -> Result<Self> {
        Ok(Self {
            estamora_spec_version: report.estamora_spec_version.clone(),
            runner: report.runner.clone(),
            issued_at: issued_at.into(),
            profile: ReceiptProfile {
                id: report.profile.id.clone(),
                version: report.profile.version.clone(),
                digest: Digest::parse(&report.profile.digest)?,
            },
            vectors: ReceiptCorpus {
                digest: Digest::parse(&report.vectors.digest)?,
                count: report.vectors.count,
            },
            target: ReceiptTarget {
                contract: report.target.contract.clone(),
                network: report.target.network.clone(),
                wasm_hash: report.target.wasm_hash.clone(),
            },
            result: ReceiptResult {
                status: report.status,
                exit_code: report.exit_code,
                summary: report.summary,
            },
            report_digest: report_digest(report)?,
        })
    }

    /// The digest of this receipt's own content.
    ///
    /// Taken over the receipt without its signature, so that signing and verifying
    /// commit to the same bytes and a signature cannot be invalidated by a re-signed
    /// copy of itself.
    ///
    /// # Errors
    ///
    /// Returns a certification error if the receipt cannot be canonicalised.
    pub fn digest(&self) -> Result<Digest> {
        let document = serde_json::to_value(self).map_err(|problem| {
            Error::new(
                ErrorClass::CertificationError,
                format!("the receipt could not be canonicalised: {problem}"),
            )
        })?;
        Digest::of_json(&document)
    }

    /// The bytes a signature covers: the receipt's own digest, as text.
    ///
    /// Signing the digest rather than the document means the signed payload is a
    /// fixed sixty-four-character phrase whatever the receipt contains, and it means
    /// a verifier can check a signature without holding the original document.
    ///
    /// # Errors
    ///
    /// As [`Receipt::digest`].
    pub fn signing_payload(&self) -> Result<String> {
        Ok(self.digest()?.as_str().to_owned())
    }
}

/// The digest of a report, over its canonical compact rendering.
///
/// # Errors
///
/// Returns a certification error if the report cannot be canonicalised.
pub fn report_digest(report: &Report) -> Result<Digest> {
    let document = serde_json::to_value(report).map_err(|problem| {
        Error::new(
            ErrorClass::CertificationError,
            format!("the report could not be canonicalised: {problem}"),
        )
    })?;
    Digest::of_json(&document)
}

/// A receipt's own digest, from its parts.
///
/// # Errors
///
/// As [`report_digest`].
pub fn receipt_digest(receipt: &Receipt) -> Result<Digest> {
    receipt.digest()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{Receipt, report_digest};
    use crate::tests::a_report;
    use estamora_core::ErrorClass;

    #[test]
    fn a_receipt_pins_the_profile_the_corpus_and_the_report() {
        let report = a_report();
        let receipt = Receipt::issue(&report, "2026-01-15T12:00:00Z").unwrap();
        assert_eq!(receipt.profile.id, "sep-41");
        assert_eq!(
            receipt.profile.digest,
            crate::digest::Digest::parse(&report.profile.digest).unwrap()
        );
        assert_eq!(receipt.vectors.count, report.vectors.count);
        assert_eq!(receipt.report_digest, report_digest(&report).unwrap());
    }

    #[test]
    fn a_report_with_a_malformed_digest_cannot_be_certified() {
        // A receipt carries digests a consumer's schema will validate, so a report
        // that could not have passed validation must not be able to produce one.
        let mut report = a_report();
        report.profile.digest = "not-a-digest".to_owned();
        let problem = Receipt::issue(&report, "2026-01-15T12:00:00Z").unwrap_err();
        assert_eq!(problem.class(), ErrorClass::CertificationError);
    }

    #[test]
    fn issuing_is_deterministic_because_it_commits_to_content() {
        let report = a_report();
        let first = Receipt::issue(&report, "2026-01-15T12:00:00Z").unwrap();
        let second = Receipt::issue(&report, "2026-01-16T00:00:00Z").unwrap();
        assert_eq!(first.report_digest, second.report_digest);
        assert_ne!(
            first.digest().unwrap(),
            second.digest().unwrap(),
            "a receipt issued at a different time is a different document"
        );
    }
}
