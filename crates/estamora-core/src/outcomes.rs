//! Conformance outcomes and the exit-code contract.
//!
//! Two things live here that the rest of the runner treats as fixed points:
//!
//! 1. The six final statuses, whose spelling is a cross-repository contract
//!    with the specification layer's report schema.
//! 2. The rule that turns a set of vector results into one of those statuses,
//!    and a status into a process exit code.
//!
//! The rule is written once, here, and tested against every branch. A runner
//! that decides a verdict in more than one place will eventually decide two
//! different verdicts for the same run.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::errors::ErrorClass;

/// The final status of a conformance run.
///
/// Only the first three describe the contract. `ExecutionError` and
/// `ProfileError` describe the run, and reporting either as a contract failure
/// is the single most damaging mistake this system could make.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConformanceStatus {
    /// Every required vector passed.
    Conformant,
    /// Some required vectors passed and some failed.
    PartiallyConformant,
    /// A required behavioural requirement was violated.
    NonConformant,
    /// The suite ran but could not reach a decision: some required vectors could
    /// not be executed, and the unexecuted ones could contain a failure.
    Inconclusive,
    /// The environment failed. The contract is not to blame and no verdict about
    /// it was reached.
    ExecutionError,
    /// The requirements were unusable, so they were never applied.
    ProfileError,
}

impl ConformanceStatus {
    /// The exact spelling the report schema enumerates.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Conformant => "CONFORMANT",
            Self::PartiallyConformant => "PARTIALLY_CONFORMANT",
            Self::NonConformant => "NON_CONFORMANT",
            Self::Inconclusive => "INCONCLUSIVE",
            Self::ExecutionError => "EXECUTION_ERROR",
            Self::ProfileError => "PROFILE_ERROR",
        }
    }

    /// Whether this status is a statement about the contract's behaviour.
    ///
    /// `Inconclusive` is not: it says the suite could not decide, which is a
    /// statement about the run.
    #[must_use]
    pub const fn describes_contract(self) -> bool {
        matches!(
            self,
            Self::Conformant | Self::PartiallyConformant | Self::NonConformant
        )
    }

    /// The process exit code for this status.
    ///
    /// A CI system reads this and nothing else, so the mapping is derived here
    /// and tested rather than assembled by the CLI:
    ///
    /// | Code | Status | What CI should do |
    /// | ---- | ------ | ----------------- |
    /// | `0`  | `CONFORMANT` | Publish or merge |
    /// | `1`  | `NON_CONFORMANT`, `PARTIALLY_CONFORMANT` | Fail the gate; the contract is at fault |
    /// | `2`  | `INCONCLUSIVE` | Fail the gate; re-run the suite |
    /// | `3`  | `PROFILE_ERROR` | Fail the build; the profile is at fault |
    /// | `4`  | `EXECUTION_ERROR` | Fail or retry; the environment is at fault |
    #[must_use]
    pub const fn exit_code(self) -> ExitCode {
        match self {
            Self::Conformant => ExitCode::Success,
            Self::NonConformant | Self::PartiallyConformant => ExitCode::NonConformant,
            Self::Inconclusive => ExitCode::Inconclusive,
            Self::ProfileError => ExitCode::SpecificationInvalid,
            Self::ExecutionError => ExitCode::EnvironmentFailed,
        }
    }

    /// Every status, in the order the report schema lists them.
    pub const ALL: [Self; 6] = [
        Self::Conformant,
        Self::PartiallyConformant,
        Self::NonConformant,
        Self::Inconclusive,
        Self::ExecutionError,
        Self::ProfileError,
    ];
}

impl fmt::Display for ConformanceStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The status of one executed vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VectorStatus {
    /// The vector's requirements were satisfied.
    Passed,
    /// The vector's requirements were violated. The contract is at fault.
    Failed,
    /// The vector could not be executed or evaluated. The contract is not at
    /// fault and the vector contributes nothing to a verdict.
    Error,
    /// The vector was deliberately not run, because a capability it needs is not
    /// configured for this run.
    Skipped,
}

impl VectorStatus {
    /// The stable machine-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Error => "error",
            Self::Skipped => "skipped",
        }
    }

    /// Whether the vector produced a verdict.
    ///
    /// `Error` and `Skipped` both mean "no verdict", which is why they are
    /// treated identically when a run is summarised: silently dropping either
    /// would let a suite that did not run report itself as clean.
    #[must_use]
    pub const fn decided(self) -> bool {
        matches!(self, Self::Passed | Self::Failed)
    }
}

impl fmt::Display for VectorStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The outcome of one evaluated assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssertionStatus {
    /// The assertion held.
    Passed,
    /// The assertion did not hold.
    Failed,
}

impl AssertionStatus {
    /// The stable machine-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }
}

impl From<bool> for AssertionStatus {
    fn from(held: bool) -> Self {
        if held { Self::Passed } else { Self::Failed }
    }
}

/// The process exit code.
///
/// A dedicated type rather than an integer, because the CLI's contract with CI
/// is that these five values mean these five things, and an `i32` returned from
/// a function is an invitation to invent a sixth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ExitCode {
    /// `0` — the run reached a conformant verdict.
    Success = 0,
    /// `1` — the contract violated a requirement.
    NonConformant = 1,
    /// `2` — the suite could not decide.
    Inconclusive = 2,
    /// `3` — the profile or the vector corpus was unusable.
    SpecificationInvalid = 3,
    /// `4` — the environment failed, so no verdict about the contract was
    /// reached.
    EnvironmentFailed = 4,
    /// `5` — the runner itself failed.
    InternalError = 5,
    /// `64` — the command line was wrong. Taken from `sysexits.h` so that a
    /// wrapper script can recognise a usage error without knowing this program.
    Usage = 64,
}

impl ExitCode {
    /// The integer a process exits with.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self as i32
    }

    /// The exit code for a failure of `class`.
    ///
    /// A failure never produces `0`, `1` or `2` unless it names the contract,
    /// which is what stops an unreachable network from being reported as
    /// non-conformance.
    #[must_use]
    pub const fn for_error_class(class: ErrorClass) -> Self {
        match class {
            ErrorClass::ProfileError | ErrorClass::VectorError => Self::SpecificationInvalid,
            ErrorClass::ContractResolutionError
            | ErrorClass::NetworkError
            | ErrorClass::ExecutionError => Self::EnvironmentFailed,
            ErrorClass::UsageError => Self::Usage,
            // An assertion failure reaching this point means the run failed to
            // classify it as a vector result first, which is a runner defect:
            // reporting the contract as non-conformant here would be a guess.
            ErrorClass::AssertionFailure
            | ErrorClass::ReportError
            | ErrorClass::CertificationError
            | ErrorClass::InternalError => Self::InternalError,
        }
    }
}

impl fmt::Display for ExitCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.as_i32())
    }
}

/// The tally a run's vector results are reduced to before a status is chosen.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunTally {
    /// Required vectors that passed.
    pub passed: u32,
    /// Required vectors that failed.
    pub failed: u32,
    /// Required vectors that could not be executed or evaluated.
    pub errored: u32,
    /// Required vectors that were not run because their environment was not
    /// configured.
    pub skipped: u32,
    /// Optional vectors that failed. Recorded, and never by itself a verdict.
    pub optional_failed: u32,
}

impl RunTally {
    /// Records a vector's status, by whether the vector was required.
    ///
    /// One place decides which counter a status increments, so a new status
    /// cannot be handled in one call site and forgotten in another.
    pub fn record(&mut self, status: VectorStatus, required: bool) {
        if !required {
            // An optional vector is never part of a verdict. A failing optional
            // vector is recorded so that the report can show it, and nothing
            // more: a profile marks a requirement optional precisely because a
            // conforming implementation may not satisfy it.
            if status == VectorStatus::Failed {
                self.optional_failed += 1;
            }
            return;
        }
        match status {
            VectorStatus::Passed => self.passed += 1,
            VectorStatus::Failed => self.failed += 1,
            VectorStatus::Error => self.errored += 1,
            VectorStatus::Skipped => self.skipped += 1,
        }
    }

    /// Required vectors that produced no verdict.
    ///
    /// Reported and skipped are counted together because both mean the same
    /// thing to a verdict: this requirement was not exercised.
    #[must_use]
    pub const fn undecided(&self) -> u32 {
        self.errored.saturating_add(self.skipped)
    }

    /// The number of required vectors considered.
    #[must_use]
    pub const fn required(&self) -> u32 {
        self.passed
            .saturating_add(self.failed)
            .saturating_add(self.errored)
            .saturating_add(self.skipped)
    }

    /// The status this tally supports.
    ///
    /// The branches, and why each is the honest answer:
    ///
    /// - A failed required vector with no passing one is `NON_CONFORMANT`: the
    ///   corpus was executed and the contract violated a requirement.
    /// - A failed required vector alongside passing ones is
    ///   `PARTIALLY_CONFORMANT`: the contract satisfies part of the profile, and
    ///   saying so is more informative than either extreme.
    /// - Any undecided required vector makes the run `INCONCLUSIVE`, even when
    ///   other vectors failed: the unexercised requirements could contain
    ///   anything, and a verdict of `NON_CONFORMANT` would imply the whole corpus
    ///   was evaluated.
    /// - Undecided vectors with nothing decided at all is `EXECUTION_ERROR`: no
    ///   statement about behaviour can be made.
    /// - Nothing undecided and nothing failed, with at least one pass, is
    ///   `CONFORMANT`.
    /// - An empty corpus is `INCONCLUSIVE`. A profile with no required vectors
    ///   is rejected at load time, so this branch is the tally's own honest
    ///   answer rather than a profile defect reported late.
    #[must_use]
    pub const fn status(&self) -> ConformanceStatus {
        if self.undecided() > 0 {
            if self.passed == 0 && self.failed == 0 {
                return ConformanceStatus::ExecutionError;
            }
            return ConformanceStatus::Inconclusive;
        }
        if self.failed > 0 {
            if self.passed == 0 {
                return ConformanceStatus::NonConformant;
            }
            return ConformanceStatus::PartiallyConformant;
        }
        if self.passed == 0 {
            return ConformanceStatus::Inconclusive;
        }
        ConformanceStatus::Conformant
    }
}

impl FromIterator<(VectorStatus, bool)> for RunTally {
    fn from_iter<I: IntoIterator<Item = (VectorStatus, bool)>>(statuses: I) -> Self {
        let mut tally = Self::default();
        for (status, required) in statuses {
            tally.record(status, required);
        }
        tally
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{AssertionStatus, ConformanceStatus, ExitCode, RunTally, VectorStatus};

    #[test]
    fn statuses_spell_exactly_what_the_report_schema_enumerates() {
        // The specification layer's report schema enumerates these strings. A
        // rename here would make a produced report fail validation there, so the
        // spellings are pinned rather than compared to `as_str` itself.
        let spellings: Vec<&str> = ConformanceStatus::ALL.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            spellings,
            vec![
                "CONFORMANT",
                "PARTIALLY_CONFORMANT",
                "NON_CONFORMANT",
                "INCONCLUSIVE",
                "EXECUTION_ERROR",
                "PROFILE_ERROR",
            ]
        );
    }

    #[test]
    fn statuses_serialize_to_their_declared_spelling() {
        for status in ConformanceStatus::ALL {
            let json = serde_json::to_string(&status).expect("a status serializes");
            assert_eq!(json, format!("\"{}\"", status.as_str()));
        }
    }

    #[test]
    fn only_three_statuses_describe_the_contract() {
        let describing: Vec<ConformanceStatus> = ConformanceStatus::ALL
            .into_iter()
            .filter(|status| status.describes_contract())
            .collect();
        assert_eq!(
            describing,
            vec![
                ConformanceStatus::Conformant,
                ConformanceStatus::PartiallyConformant,
                ConformanceStatus::NonConformant,
            ]
        );
    }

    #[test]
    fn an_execution_error_never_exits_like_a_conformance_failure() {
        // The whole point of the taxonomy: a network outage must not be
        // reported to CI as a contract that fails its profile.
        assert_eq!(
            ConformanceStatus::ExecutionError.exit_code(),
            ExitCode::EnvironmentFailed
        );
        assert_ne!(
            ConformanceStatus::ExecutionError.exit_code(),
            ConformanceStatus::NonConformant.exit_code()
        );
    }

    #[test]
    fn a_profile_defect_is_not_reported_as_a_contract_failure() {
        assert_eq!(
            ConformanceStatus::ProfileError.exit_code(),
            ExitCode::SpecificationInvalid
        );
    }

    #[test]
    fn exit_codes_are_the_documented_integers() {
        assert_eq!(ExitCode::Success.as_i32(), 0);
        assert_eq!(ExitCode::NonConformant.as_i32(), 1);
        assert_eq!(ExitCode::Inconclusive.as_i32(), 2);
        assert_eq!(ExitCode::SpecificationInvalid.as_i32(), 3);
        assert_eq!(ExitCode::EnvironmentFailed.as_i32(), 4);
        assert_eq!(ExitCode::InternalError.as_i32(), 5);
        assert_eq!(ExitCode::Usage.as_i32(), 64);
    }

    #[test]
    fn a_failure_that_names_the_contract_never_exits_successfully() {
        use crate::errors::ErrorClass;
        for class in ErrorClass::ALL {
            let code = ExitCode::for_error_class(class).as_i32();
            assert_ne!(code, 0, "{class} must not exit successfully");
            if class.blames_contract() {
                // Assertion failures are classified as vector results before they
                // reach an exit code; if one arrives here it is a runner defect.
                assert_eq!(code, ExitCode::InternalError.as_i32());
            }
        }
    }

    #[test]
    fn a_clean_run_that_ran_nothing_is_inconclusive() {
        let tally = RunTally::default();
        assert_eq!(tally.status(), ConformanceStatus::Inconclusive);
        assert_eq!(tally.required(), 0);
    }

    #[test]
    fn a_fully_passing_corpus_is_conformant() {
        let tally =
            RunTally::from_iter([(VectorStatus::Passed, true), (VectorStatus::Passed, true)]);
        assert_eq!(tally.status(), ConformanceStatus::Conformant);
        assert_eq!(tally.status().exit_code(), ExitCode::Success);
    }

    #[test]
    fn a_single_failure_among_passes_is_partially_conformant() {
        let tally =
            RunTally::from_iter([(VectorStatus::Passed, true), (VectorStatus::Failed, true)]);
        assert_eq!(tally.status(), ConformanceStatus::PartiallyConformant);
        assert_eq!(tally.status().exit_code(), ExitCode::NonConformant);
    }

    #[test]
    fn a_corpus_that_fails_entirely_is_non_conformant() {
        let tally =
            RunTally::from_iter([(VectorStatus::Failed, true), (VectorStatus::Failed, true)]);
        assert_eq!(tally.status(), ConformanceStatus::NonConformant);
    }

    #[test]
    fn an_unexecuted_requirement_makes_a_failing_run_inconclusive() {
        // The distinction that stops a partially executed suite from claiming a
        // verdict: the vectors that could not run might have failed too, so
        // NON_CONFORMANT would overstate what was established.
        let partially_executed = RunTally::from_iter([
            (VectorStatus::Failed, true),
            (VectorStatus::Error, true),
            (VectorStatus::Passed, true),
        ]);
        assert_eq!(partially_executed.status(), ConformanceStatus::Inconclusive);
        assert_eq!(
            partially_executed.status().exit_code(),
            ExitCode::Inconclusive
        );
    }

    #[test]
    fn an_unexecuted_requirement_stops_a_passing_run_from_being_conformant() {
        for undecided in [VectorStatus::Error, VectorStatus::Skipped] {
            let tally = RunTally::from_iter([(VectorStatus::Passed, true), (undecided, true)]);
            assert_eq!(
                tally.status(),
                ConformanceStatus::Inconclusive,
                "a {undecided} requirement must not yield CONFORMANT"
            );
        }
    }

    #[test]
    fn a_corpus_that_ran_nothing_but_errored_is_an_execution_error() {
        let tally = RunTally::from_iter([(VectorStatus::Error, true), (VectorStatus::Error, true)]);
        assert_eq!(tally.status(), ConformanceStatus::ExecutionError);
        assert!(!tally.status().describes_contract());
    }

    #[test]
    fn an_optional_failure_is_recorded_without_deciding_the_run() {
        let tally =
            RunTally::from_iter([(VectorStatus::Passed, true), (VectorStatus::Failed, false)]);
        assert_eq!(tally.optional_failed, 1);
        assert_eq!(tally.failed, 0);
        assert_eq!(tally.status(), ConformanceStatus::Conformant);
    }

    #[test]
    fn assertion_status_maps_from_a_boolean() {
        assert_eq!(AssertionStatus::from(true), AssertionStatus::Passed);
        assert_eq!(AssertionStatus::from(false), AssertionStatus::Failed);
    }

    #[test]
    fn vector_statuses_report_whether_a_verdict_was_reached() {
        assert!(VectorStatus::Passed.decided());
        assert!(VectorStatus::Failed.decided());
        assert!(!VectorStatus::Error.decided());
        assert!(!VectorStatus::Skipped.decided());
    }

    #[test]
    fn undecided_requirements_are_reported_and_skipped_together() {
        let tally = RunTally::from_iter([
            (VectorStatus::Error, true),
            (VectorStatus::Skipped, true),
            (VectorStatus::Passed, true),
        ]);
        assert_eq!(tally.undecided(), 2);
        assert_eq!(tally.required(), 3);
    }
}
