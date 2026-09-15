//! What a vector concluded, per assertion.
//!
//! The shape mirrors the specification's report schema, because the report is the
//! contract this runner publishes: an independent implementation must be able to
//! emit a document Estamora tooling accepts, and a runner whose internal model
//! differed from the published one would translate between them and eventually
//! translate something wrong.
//!
//! # Results are per assertion, never per vector
//!
//! Interface compatibility, authorization, events, behaviour, state and invariants
//! are independent claims about a contract. Collapsing them into one boolean makes
//! a caller unable to see *which* of them failed, and lets a passing interface
//! section be presented as evidence of correct behaviour — which is exactly the
//! confusion this project exists to remove. So every check is reported on its own,
//! with the expected and observed values retained as text.

use estamora_core::{AssertionStatus, VectorStatus};
use estamora_vectors::AssertionCategory;

/// One reported check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionOutcome {
    /// The rule's identifier, reported verbatim so a failure names what was
    /// violated.
    pub id: String,
    /// Which conformance dimension the check belongs to.
    pub category: AssertionCategory,
    /// Whether the check held.
    pub status: AssertionStatus,
    /// What the requirement asked for.
    pub expected: String,
    /// What was observed.
    pub observed: String,
    /// Anything further a reader needs.
    pub detail: Option<String>,
}

impl AssertionOutcome {
    /// A check that held.
    #[must_use]
    pub fn passed(
        id: impl Into<String>,
        category: AssertionCategory,
        expected: impl Into<String>,
        observed: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            category,
            status: AssertionStatus::Passed,
            expected: expected.into(),
            observed: observed.into(),
            detail: None,
        }
    }

    /// A check that did not hold.
    #[must_use]
    pub fn failed(
        id: impl Into<String>,
        category: AssertionCategory,
        expected: impl Into<String>,
        observed: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            category,
            status: AssertionStatus::Failed,
            expected: expected.into(),
            observed: observed.into(),
            detail: None,
        }
    }

    /// A check whose result is a plain boolean, with both sides rendered.
    ///
    /// For the checks whose expectation is structural rather than algebraic —
    /// whether a method is present, whether a parameter count matches — so that
    /// they are reported through the same shape as every other check rather than
    /// through a boolean a caller could collapse.
    #[must_use]
    pub fn from_boolean(
        id: impl Into<String>,
        category: AssertionCategory,
        expected: impl Into<String>,
        observed: impl Into<String>,
        held: bool,
    ) -> Self {
        Self {
            id: id.into(),
            category,
            status: AssertionStatus::from(held),
            expected: expected.into(),
            observed: observed.into(),
            detail: None,
        }
    }

    /// A check whose result came from an evaluation.
    #[must_use]
    pub fn from_evaluation(
        id: impl Into<String>,
        category: AssertionCategory,
        evaluation: &crate::eval::Evaluation,
    ) -> Self {
        Self {
            id: id.into(),
            category,
            status: AssertionStatus::from(evaluation.held),
            expected: evaluation.expected.clone(),
            observed: evaluation.observed.clone(),
            detail: None,
        }
    }

    /// Attaches further explanation.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// A finding that is not itself a check.
///
/// Carries a stable code so a consumer can match on it without reading the prose,
/// which is what lets `estamora-runner` grow new findings without breaking a
/// downstream integration that keys off them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutcomeDiagnostic {
    /// A stable short code.
    pub code: String,
    /// What was found.
    pub message: String,
}

impl OutcomeDiagnostic {
    /// A finding.
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// What one vector concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorOutcome {
    /// The vector's identifier.
    pub vector_id: String,
    /// The class of property the vector probes.
    pub category: String,
    /// Whether a verdict was reached.
    pub status: VectorStatus,
    /// Every check that was reported.
    pub assertions: Vec<AssertionOutcome>,
    /// Findings that are not checks.
    pub diagnostics: Vec<OutcomeDiagnostic>,
}

impl VectorOutcome {
    /// An outcome from its parts.
    #[must_use]
    pub fn new(
        vector_id: impl Into<String>,
        category: impl Into<String>,
        status: VectorStatus,
        assertions: Vec<AssertionOutcome>,
    ) -> Self {
        Self {
            vector_id: vector_id.into(),
            category: category.into(),
            status,
            assertions,
            diagnostics: Vec::new(),
        }
    }

    /// A vector that could not be executed, with the reason.
    ///
    /// `Error` rather than `Failed`: the contract is not at fault and the vector
    /// contributes nothing to a verdict. A run that treated this as a failure would
    /// blame a contract for the runner's inability to measure it.
    #[must_use]
    pub fn could_not_run(
        vector_id: impl Into<String>,
        category: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            vector_id: vector_id.into(),
            category: category.into(),
            status: VectorStatus::Error,
            assertions: Vec::new(),
            diagnostics: vec![OutcomeDiagnostic::new(code, message)],
        }
    }

    /// Attaches a finding.
    #[must_use]
    pub fn with_diagnostic(mut self, diagnostic: OutcomeDiagnostic) -> Self {
        self.diagnostics.push(diagnostic);
        self
    }

    /// Attaches several findings.
    #[must_use]
    pub fn with_diagnostics(mut self, diagnostics: Vec<OutcomeDiagnostic>) -> Self {
        self.diagnostics.extend(diagnostics);
        self
    }

    /// How many checks of `category` held, and how many there were.
    #[must_use]
    pub fn tally(&self, category: AssertionCategory) -> (u32, u32) {
        let matching = self
            .assertions
            .iter()
            .filter(|assertion| assertion.category == category);
        // Saturating rather than truncating: a tally that wrapped would report a
        // smaller corpus than the one that ran, and a summary understating its own
        // coverage is the failure mode this report exists to prevent.
        let total = u32::try_from(matching.clone().count()).unwrap_or(u32::MAX);
        let passed = u32::try_from(
            matching
                .filter(|assertion| assertion.status == AssertionStatus::Passed)
                .count(),
        )
        .unwrap_or(u32::MAX);
        (passed, total)
    }

    /// Every check that did not hold.
    #[must_use]
    pub fn failures(&self) -> Vec<&AssertionOutcome> {
        self.assertions
            .iter()
            .filter(|assertion| assertion.status == AssertionStatus::Failed)
            .collect()
    }

    /// Whether every reported check held.
    #[must_use]
    pub fn all_held(&self) -> bool {
        self.failures().is_empty()
    }
}
