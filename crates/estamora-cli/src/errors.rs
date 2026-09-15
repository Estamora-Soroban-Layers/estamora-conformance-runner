//! The process exit status, and why it is derived rather than chosen.
//!
//! A CI system reads one thing from this program: the number it exits with. So the
//! mapping from a failure to a status is written once, here, and is derived from the
//! failure's [`estamora_core::ErrorClass`] rather than assembled at each `return`. The rule it
//! enforces is the reason the taxonomy exists at all:
//!
//! | Status | Exit | Meaning |
//! | ------ | ---- | ------- |
//! | `CONFORMANT` | 0 | The contract satisfied every required vector |
//! | `NON_CONFORMANT`, `PARTIALLY_CONFORMANT` | 1 | The contract violated a requirement |
//! | `INCONCLUSIVE` | 2 | Part of the corpus could not be decided; re-run |
//! | `PROFILE_ERROR` | 3 | The requirements were unusable; the profile is at fault |
//! | `EXECUTION_ERROR` | 4 | The environment failed; the contract is not at fault |
//! | `INTERNAL_ERROR` | 5 | The runner failed |
//! | — | 64 | The command line was wrong |
//!
//! The three that matter most are separated on purpose. A network outage must not
//! block a release as though the contract had failed, and a malformed profile must
//! not be reported as a contract defect. Exiting `1` for an unreachable endpoint is
//! the single most damaging mistake this program could make, because it teaches its
//! users to distrust every `1` it produces.

use estamora_core::{Error, ExitCode};

/// The exit status a failure of `class` produces.
///
/// Delegates to [`ExitCode::for_error_class`] so that the rule lives in
/// `estamora-core` beside the error registry it is derived from, rather than being
/// restated here where it could drift.
#[must_use]
pub const fn exit_code(class: estamora_core::ErrorClass) -> ExitCode {
    ExitCode::for_error_class(class)
}

/// The exit status for a failure.
#[must_use]
pub fn exit_code_for(error: &Error) -> ExitCode {
    exit_code(error.class())
}

/// The exit status for a run that reached a verdict.
///
/// The verdict is reduced to a status by `estamora-core` and the status to a code by
/// the same module, so a run and a failure cannot disagree about what a number means.
#[must_use]
pub const fn exit_code_for_status(status: estamora_core::ConformanceStatus) -> ExitCode {
    status.exit_code()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{exit_code, exit_code_for};
    use estamora_core::{Error, ErrorClass, ExitCode};

    #[test]
    fn the_environment_failing_never_looks_like_the_contract_failing() {
        for class in [
            ErrorClass::NetworkError,
            ErrorClass::ExecutionError,
            ErrorClass::ContractResolutionError,
        ] {
            let code = exit_code(class).as_i32();
            assert_ne!(code, 0, "{class} must not exit successfully");
            assert_ne!(
                code,
                ExitCode::NonConformant.as_i32(),
                "{class} must not be reported to CI as a conformance failure"
            );
            assert_eq!(code, ExitCode::EnvironmentFailed.as_i32());
        }
    }

    #[test]
    fn an_unusable_profile_is_reported_as_the_profile_s_fault() {
        for class in [ErrorClass::ProfileError, ErrorClass::VectorError] {
            assert_eq!(exit_code(class), ExitCode::SpecificationInvalid);
        }
    }

    #[test]
    fn every_class_produces_a_code_a_script_can_act_on() {
        // No class may map to success, and none may map to `1`: a class that reached
        // this point without being reduced to a vector result is a runner defect, and
        // blaming the contract for it would be a guess.
        for class in ErrorClass::ALL {
            let code = exit_code(class).as_i32();
            assert_ne!(code, 0, "{class} must not exit successfully");
            assert_ne!(code, 1, "{class} must not name the contract");
        }
    }

    #[test]
    fn a_failure_is_mapped_by_its_class_and_not_by_its_message() {
        let error = Error::new(ErrorClass::NetworkError, "could not reach the endpoint");
        assert_eq!(exit_code_for(&error), ExitCode::EnvironmentFailed);
    }
}
