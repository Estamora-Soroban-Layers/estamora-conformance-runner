//! The runner's error taxonomy.
//!
//! Every failure the runner can report has two independent properties: a
//! *class*, which says what went wrong, and a *blame*, which says who is
//! answerable for it. The distinction is the reason this module exists.
//!
//! A runner that reports "the contract is non-conformant" when the truth is
//! "the RPC endpoint was unreachable" is worse than useless: it blocks a release
//! for a network outage and teaches its users to distrust every verdict it
//! produces. So the taxonomy is closed, the blame is derived from the class
//! rather than chosen at each call site, and the exit code that a CI system
//! reads is derived from both.

use std::error::Error as StdError;
use std::fmt;

use serde::{Deserialize, Serialize};

/// What went wrong.
///
/// The registry is closed. A new failure mode is a new variant, reviewed
/// alongside the report it will appear in, rather than a free-form string that
/// each call site invents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorClass {
    /// A profile bundle is malformed, self-inconsistent or names something that
    /// does not exist. The requirements were never usable, so no verdict about
    /// any contract was reached.
    ProfileError,
    /// A vector is malformed, or disagrees with the profile it is declared
    /// against. As with a profile, no verdict was reached.
    VectorError,
    /// The target contract could not be resolved: an unknown network, an
    /// identifier that does not exist there, or a fixture that could not be
    /// built.
    ContractResolutionError,
    /// The network or execution environment failed: a refused connection, a
    /// timeout, or a node that returned an error rather than a result.
    NetworkError,
    /// The environment reached the contract but could not produce an
    /// observation the runner could evaluate.
    ExecutionError,
    /// The contract ran, was observed, and violated a requirement. This is the
    /// only class that describes the contract.
    AssertionFailure,
    /// A result could not be rendered: bad configuration, an unwritable path, or
    /// a serialization failure.
    ReportError,
    /// A receipt could not be produced or verified.
    CertificationError,
    /// The command line was wrong: an unknown flag, a missing argument, a
    /// combination that cannot be satisfied.
    UsageError,
    /// The runner failed in a way that indicates a defect in the runner itself.
    /// It is reported rather than panicking so that a CI job gets a usable
    /// message and a stable exit code.
    InternalError,
}

impl ErrorClass {
    /// The stable machine-readable name, matching the report schema.
    ///
    /// These strings are a cross-repository contract: the specification layer's
    /// report schema enumerates the same vocabulary, so a result produced here
    /// validates against it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProfileError => "PROFILE_ERROR",
            Self::VectorError => "VECTOR_ERROR",
            Self::ContractResolutionError => "CONTRACT_RESOLUTION_ERROR",
            Self::NetworkError => "NETWORK_ERROR",
            Self::ExecutionError => "EXECUTION_ERROR",
            Self::AssertionFailure => "ASSERTION_FAILURE",
            Self::ReportError => "REPORT_ERROR",
            Self::CertificationError => "CERTIFICATION_ERROR",
            Self::UsageError => "USAGE_ERROR",
            Self::InternalError => "INTERNAL_ERROR",
        }
    }

    /// Who is answerable for the failure.
    ///
    /// Derived from the class rather than passed in, because a call site that
    /// could choose the blame is a call site that can blame the wrong party.
    #[must_use]
    pub const fn blame(self) -> Blame {
        match self {
            Self::ProfileError | Self::VectorError => Blame::Specification,
            Self::AssertionFailure => Blame::Contract,
            Self::ContractResolutionError | Self::NetworkError | Self::ExecutionError => {
                Blame::Environment
            },
            Self::ReportError | Self::CertificationError | Self::InternalError => Blame::Runner,
            Self::UsageError => Blame::Invocation,
        }
    }

    /// Whether a failure of this class says anything about the contract.
    ///
    /// Only [`Blame::Contract`] does. It is a method rather than a comparison at
    /// each call site so that the rule is stated once and tested.
    #[must_use]
    pub const fn blames_contract(self) -> bool {
        matches!(self.blame(), Blame::Contract)
    }

    /// Every class, in the order the report schema lists them.
    ///
    /// Used by tests that pin the registry, so that adding a variant without
    /// documenting it fails the build.
    pub const ALL: [Self; 10] = [
        Self::ProfileError,
        Self::VectorError,
        Self::ContractResolutionError,
        Self::NetworkError,
        Self::ExecutionError,
        Self::AssertionFailure,
        Self::ReportError,
        Self::CertificationError,
        Self::UsageError,
        Self::InternalError,
    ];
}

impl fmt::Display for ErrorClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Who is answerable for a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Blame {
    /// The contract's behaviour. The only blame that supports a non-conformant
    /// verdict.
    Contract,
    /// The profile or vector corpus supplied for the run.
    Specification,
    /// The network, the node, or the execution host.
    Environment,
    /// The person or system that invoked the runner.
    Invocation,
    /// The runner itself.
    Runner,
}

impl Blame {
    /// The stable machine-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Contract => "contract",
            Self::Specification => "specification",
            Self::Environment => "environment",
            Self::Invocation => "invocation",
            Self::Runner => "runner",
        }
    }
}

impl fmt::Display for Blame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A failure, carrying its class, a message, and whatever context the call site
/// could supply.
///
/// This is a struct rather than an enum of variants because the class registry
/// is closed while the messages are not: two `EXECUTION_ERROR`s from different
/// parts of the pipeline share a class and a blame but need different prose, and
/// a variant per message would turn the class registry back into an open set.
#[derive(Debug)]
pub struct Error {
    class: ErrorClass,
    message: String,
    context: Vec<(String, String)>,
    source: Option<Box<dyn StdError + Send + Sync + 'static>>,
}

impl Error {
    /// Builds an error of `class` with `message`.
    ///
    /// The message is written for a person reading a CI log, so it should say
    /// what was expected and what was found rather than restating the class.
    #[must_use]
    pub fn new(class: ErrorClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
            context: Vec::new(),
            source: None,
        }
    }

    /// Attaches a key and value that place the failure.
    ///
    /// Context is rendered in order in the report, so keys are added outermost
    /// last to read as a path: `vector`, then `profile`, then `path`.
    #[must_use]
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.push((key.into(), value.into()));
        self
    }

    /// Attaches the underlying cause.
    #[must_use]
    pub fn with_source(mut self, source: impl StdError + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// The class of this failure.
    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        self.class
    }

    /// Who is answerable for this failure.
    #[must_use]
    pub const fn blame(&self) -> Blame {
        self.class.blame()
    }

    /// Whether this failure says anything about the contract.
    #[must_use]
    pub const fn blames_contract(&self) -> bool {
        self.class.blames_contract()
    }

    /// The human-readable message, without context or cause.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The context pairs, in the order they were added.
    #[must_use]
    pub fn context(&self) -> &[(String, String)] {
        &self.context
    }

    /// One context value, by key.
    ///
    /// The accessor a caller uses when it has to branch on *why* a failure of a
    /// class happened — distinguishing a receipt whose signature did not check out
    /// from one whose report was swapped — without matching on prose, which would
    /// break the moment a message was reworded.
    #[must_use]
    pub fn context_value(&self, key: &str) -> Option<&str> {
        self.context
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// The message with its context rendered as a suffix, for a log line or a
    /// terminal report.
    ///
    /// The structured form is what reports serialize; this is the single-string
    /// form for a human reading a terminal, and it is deliberately the only
    /// place the two are joined.
    #[must_use]
    pub fn display_chain(&self) -> String {
        let mut rendered = self.message.clone();
        for (key, value) in &self.context {
            rendered.push_str("\n  ");
            rendered.push_str(key);
            rendered.push_str(": ");
            rendered.push_str(value);
        }
        if let Some(source) = &self.source {
            rendered.push_str("\n  caused by: ");
            rendered.push_str(&source.to_string());
        }
        rendered
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `Display` prints the message and the context, because every place that
        // formats an error instead of serializing it is writing a log line.
        formatter.write_str(&self.display_chain())
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_ref()
            .map(|source| source.as_ref() as &(dyn StdError + 'static))
    }
}

impl From<std::io::Error> for Error {
    fn from(source: std::io::Error) -> Self {
        Self::new(ErrorClass::InternalError, format!("I/O failure: {source}")).with_source(source)
    }
}

/// The runner's result type.
pub type Result<T> = std::result::Result<T, Error>;

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The run cannot produce a verdict.
    Error,
    /// The run produces a verdict, and the finding is recorded with it. A
    /// warning never turns a passing run into a failing one; the profile decides
    /// that by marking an invariant or requirement.
    Warning,
}

impl Severity {
    /// The stable machine-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

/// A finding that is not by itself a verdict.
///
/// Diagnostics are accumulated rather than returned on the first problem, so
/// that one run reports every defect in a corpus instead of one per attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// How serious the finding is.
    pub severity: Severity,
    /// What kind of failure it is, drawn from the same closed registry.
    pub class: ErrorClass,
    /// What was expected and what was found.
    pub message: String,
    /// Where it was found, as `key: value` pairs in outermost-last order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<(String, String)>,
}

impl Diagnostic {
    /// Records an error-severity finding.
    #[must_use]
    pub fn error(class: ErrorClass, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            class,
            message: message.into(),
            context: Vec::new(),
        }
    }

    /// Records a warning-severity finding.
    #[must_use]
    pub fn warning(class: ErrorClass, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            class,
            message: message.into(),
            context: Vec::new(),
        }
    }

    /// Attaches a key and value that place the finding.
    #[must_use]
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.push((key.into(), value.into()));
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.class, self.message)?;
        for (key, value) in &self.context {
            write!(formatter, "\n  {key}: {value}")?;
        }
        Ok(())
    }
}

/// A collection of diagnostics.
///
/// Validators fill one of these and then ask it for a verdict, which is what
/// keeps "report everything you found" and "fail if anything is wrong" from
/// being two separate code paths that can disagree.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostics {
    findings: Vec<Diagnostic>,
}

impl Diagnostics {
    /// An empty collection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a finding.
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.findings.push(diagnostic);
    }

    /// Records an error-severity finding and returns it for further decoration.
    pub fn error(&mut self, class: ErrorClass, message: impl Into<String>) -> &mut Diagnostic {
        self.findings.push(Diagnostic::error(class, message));
        // The index is computed after the push, so it addresses the finding just
        // added and cannot be out of range.
        let index = self.findings.len() - 1;
        &mut self.findings[index]
    }

    /// Records a warning-severity finding.
    pub fn warning(&mut self, class: ErrorClass, message: impl Into<String>) {
        self.findings.push(Diagnostic::warning(class, message));
    }

    /// Whether any finding of error severity was recorded.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == Severity::Error)
    }

    /// The number of error-severity findings.
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Error)
            .count()
    }

    /// The number of warning-severity findings.
    #[must_use]
    pub fn warning_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Warning)
            .count()
    }

    /// Every finding, in the order it was recorded.
    #[must_use]
    pub fn findings(&self) -> &[Diagnostic] {
        &self.findings
    }

    /// Whether nothing at all was recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }

    /// Takes the findings, leaving the collection empty.
    #[must_use]
    pub fn into_findings(self) -> Vec<Diagnostic> {
        self.findings
    }
}

impl From<Diagnostic> for Diagnostics {
    fn from(diagnostic: Diagnostic) -> Self {
        Self {
            findings: vec![diagnostic],
        }
    }
}

/// Converts a collection of error-severity findings into the error they
/// describe, or `Ok(())` when there are none.
///
/// This is the single place the two representations meet, so that a validator
/// cannot report a clean result while holding an error.
///
/// # Errors
///
/// Returns the first error-severity finding as an [`Error`] of that finding's
/// class, with the total number of defects attached as context. A caller that
/// needs the individual findings should report them before calling this.
pub fn into_result(diagnostics: &Diagnostics) -> Result<()> {
    if !diagnostics.has_errors() {
        return Ok(());
    }
    let first = diagnostics
        .findings()
        .iter()
        .find(|finding| finding.severity == Severity::Error);
    let (class, message) = match first {
        Some(finding) => (
            finding.class,
            format!(
                "{} defect(s) found; first: {}",
                diagnostics.error_count(),
                finding.message
            ),
        ),
        None => (
            ErrorClass::InternalError,
            "diagnostics reported errors without recording one".to_owned(),
        ),
    };
    Err(Error::new(class, message)
        .with_context("error_count", diagnostics.error_count().to_string()))
}
