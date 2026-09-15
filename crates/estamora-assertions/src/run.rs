//! Evaluating one vector end to end.
//!
//! This is the point at which Estamora answers its question for one scenario: given
//! a vector, a profile, a world before and after, and an observation of the call,
//! what did the contract do and which requirements did it satisfy?
//!
//! # Order is not incidental
//!
//! The dimensions are evaluated in a fixed order, and two of the orderings are
//! requirements rather than preferences:
//!
//! 1. **Interface first**, because a refusal is only readable once the method is
//!    known to exist. Every later dimension that reads an abort consults the result.
//! 2. **Behaviour before invariants**, because a behavioural rule names the
//!    invariants it requires, and those identifiers have to be collected before the
//!    invariant dimension can be told which rules apply.
//!
//! # An undecidable vector is never a failed one
//!
//! Any dimension that cannot be evaluated makes the vector `error`, not `failed`.
//! The contract is not at fault when the runner could not measure it, and a run that
//! blamed the contract would be worse than a run that produced no result at all.
//!
//! # The interface checks are attached to every vector
//!
//! Interface compatibility is a property of the contract rather than of a scenario,
//! so its checks are the same for every vector in a run. They are reported with each
//! result because the report's shape attaches assertions to vector results, and the
//! report summary counts *distinct* assertion identifiers — taking the worst result
//! each identifier reached — so that the repetition is not read as a larger body of
//! evidence than it is, and a check that failed in one scenario is never hidden
//! behind the same identifier passing in another.
//!
//! # A requirement the runner could not observe is a finding, not a failure
//!
//! Some requirements are decidable only from an observation the environment did not
//! produce — the principals a contract demanded cannot be read from a call that was
//! refused. Recording that as a failed check would blame the contract for the
//! runner's blind spot, so it is recorded as a diagnostic on the vector instead, and
//! the vector's verdict rests on the checks that were actually made.

use std::fmt;

use estamora_core::Result;
use estamora_core::VectorStatus;
use estamora_profile::ProfileBundle;
use estamora_vectors::VectorDocument;

use crate::dimensions::{authorization, behavior, events, failures, interface, invariants, state};
use crate::eval::Environment;
use crate::observation::ObservedCall;
use crate::outcome::{AssertionOutcome, VectorOutcome};
use crate::world::World;

/// Prints what identifies the evaluation, never the worlds themselves.
///
/// A `World` is a handle onto an execution environment, and rendering one would put
/// a value that differs between two runs of the same vector into every diagnostic —
/// the opposite of what this runner produces anywhere else.
impl fmt::Debug for RunObservation<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunObservation")
            .field("profile", &self.profile.document().profile.id)
            .field("vector", &self.vector.id)
            .field("call", &self.call)
            .field("interface", &self.interface)
            .finish_non_exhaustive()
    }
}

/// Everything one evaluation needs.
pub struct RunObservation<'a> {
    /// The profile the vector is executed against.
    pub profile: &'a ProfileBundle,
    /// The vector under test.
    pub vector: &'a VectorDocument,
    /// The world the operation started in, for relative comparisons.
    pub before: &'a dyn World,
    /// The world it ended in.
    pub after: &'a dyn World,
    /// What was observed about the call.
    pub call: ObservedCall,
    /// What was learned about the contract's interface.
    pub interface: interface::InterfaceInspection,
}

/// Evaluates one vector and returns everything it concluded.
///
/// # Errors
///
/// Returns an error only when the run could not be attempted at all — a corpus
/// reference to something the profile does not declare. Everything else is reported
/// inside the returned outcome, because a vector that could not be evaluated is a
/// result about the run rather than a failure of the function.
pub fn evaluate(observation: &RunObservation<'_>) -> Result<VectorOutcome> {
    let vector = observation.vector;
    let mut call = observation.call.clone();
    // One place decides whether a refusal is readable. A call site that could set
    // this itself would eventually set it wrongly, and a wrongly-verified refusal
    // satisfies every requirement for a refusal.
    call.interface_verified = interface::observed_verifies_method(&call, &observation.interface);

    let ledger_sequence = vector.fixtures.ledger.sequence;
    let start = Environment {
        before: observation.before,
        after: observation.before,
        inputs: &vector.inputs,
        ledger_sequence,
    };
    let end = Environment {
        before: observation.before,
        after: observation.after,
        inputs: &vector.inputs,
        ledger_sequence,
    };

    let mut assertions: Vec<AssertionOutcome> = Vec::new();
    let mut diagnostics = Vec::new();

    // 1. Interface. A failure here does not stop the other dimensions: a contract
    //    with one missing method can still be conformant on the behaviour of
    //    everything else, and reporting only the first defect would understate
    //    what was measured.
    match interface::evaluate(
        &observation.profile.methods().methods,
        &observation.interface,
    ) {
        Ok(outcomes) => assertions.extend(outcomes),
        Err(problem) => {
            return Ok(VectorOutcome::could_not_run(
                vector.id.clone(),
                vector.kind.as_str(),
                "interface-unavailable",
                problem.to_string(),
            ));
        },
    }

    // 2. Behaviour, which also tells the invariant dimension which rules applied.
    let behavior = behavior::evaluate(observation.profile, vector, &start, &end, &call)?;
    for (rule, reason) in &behavior.inapplicable {
        diagnostics.push(crate::outcome::OutcomeDiagnostic::new(
            format!("rule-inapplicable.{rule}"),
            reason.clone(),
        ));
    }
    assertions.extend(behavior.assertions);

    // 3. Authorization.
    let authorization = authorization::evaluate(observation.profile, vector, &end, &call)?;
    for (code, reason) in &authorization.unobservable {
        diagnostics.push(crate::outcome::OutcomeDiagnostic::new(
            code.clone(),
            reason.clone(),
        ));
    }
    assertions.extend(authorization.assertions);

    // 4. Events.
    assertions.extend(events::evaluate(observation.profile, vector, &end, &call)?);

    // 5. State.
    assertions.extend(state::evaluate(vector, &end)?);

    // 6. Invariants.
    let mut invariants = vector.expected.invariants.clone();
    for invariant in &behavior.invariants {
        if !invariants.contains(invariant) {
            invariants.push(invariant.clone());
        }
    }
    let invariant_report = invariants::evaluate(
        observation.profile,
        vector,
        &invariants,
        &end,
        observation.before,
        observation.after,
        &call,
    )?;
    assertions.extend(invariant_report.assertions);
    diagnostics.extend(invariant_report.warnings);

    // 7. Failure.
    assertions.extend(failures::evaluate(
        observation.profile,
        vector,
        &call,
        observation.before,
        observation.after,
    )?);

    let status = if assertions
        .iter()
        .any(|assertion| assertion.status == estamora_core::AssertionStatus::Failed)
    {
        VectorStatus::Failed
    } else {
        VectorStatus::Passed
    };

    Ok(
        VectorOutcome::new(vector.id.clone(), vector.kind.as_str(), status, assertions)
            .with_diagnostics(diagnostics),
    )
}
