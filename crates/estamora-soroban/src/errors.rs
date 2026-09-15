//! Failure construction for the Soroban layer.
//!
//! Every constructor here names an [`ErrorClass`] from the core taxonomy, so that
//! a failure raised while talking to a ledger is classified where it happens
//! rather than at the point it is printed. A network or host failure must never
//! be able to reach the report as a contract failure, and the only way to
//! guarantee that is for the classification to be made once, here.

use estamora_core::{Error, ErrorClass};

/// A value observed on the ledger could not be represented in the runner's own
/// types.
///
/// This is an `EXECUTION_ERROR` rather than an assertion failure: the runner
/// could not form an observation, so it has nothing to assert against and must
/// not imply that the contract did anything wrong.
pub(crate) fn observation_error(message: impl Into<String>) -> Error {
    Error::new(
        ErrorClass::ExecutionError,
        format!(
            "the observation could not be represented: {}",
            message.into()
        ),
    )
}
