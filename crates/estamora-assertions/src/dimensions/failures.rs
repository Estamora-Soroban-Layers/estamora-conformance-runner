//! The failure dimension.
//!
//! Negative testing is mandatory: a contract that silently succeeds where the
//! standard requires a rejection is the most dangerous kind of non-conformance,
//! because no happy-path test can detect it. So this dimension is not satisfied by
//! "something went wrong". It checks four independent things — that the call was
//! refused at all, by a signal the profile permits, leaving the state it found, and
//! emitting nothing the profile says must not be emitted — and reports each
//! separately.
//!
//! # The three outcomes that must not be confused
//!
//! | Observed | Meaning | Reported as |
//! | -------- | ------- | ----------- |
//! | The call returned | The contract accepted what must be refused | `UNEXPECTED_SUCCESS` — a failure |
//! | The call aborted after interface inspection | The contract refused | A result to be checked |
//! | The call aborted with no inspection, or could not run | Nothing is known | An error, never a failure |
//!
//! # Why the payload is not pinned
//!
//! Soroban signals failure by trapping, and the trap payload is
//! implementation-defined. Pinning an exact error symbol would reject conforming
//! contracts that chose a different one, which is how a specification turns into a
//! compatibility test for a single implementation. Estamora standardises the
//! semantic *category* and records the payload, rather than requiring one.
//!
//! # Why the state effect is checked separately from the signal
//!
//! A contract can refuse correctly and still have written to storage before its
//! check ran. The signal says the call was refused; the state effect says whether
//! the refusal was clean. They are independent, and a profile that stated only the
//! first would accept a contract whose refused calls are indistinguishable from
//! accepted ones to anyone reading storage.

use estamora_core::{Error, ErrorClass, Result};
use estamora_profile::failures::{FailureDefinition, FailureSignal, StateEffect};
use estamora_profile::{EventsEmitted, ProfileBundle};
use estamora_vectors::{AssertionCategory, ExpectedResult, VectorDocument};

use crate::observation::{CallResult, ObservedCall};
use crate::outcome::AssertionOutcome;
use crate::world::World;

use super::state;

/// The signal a refusal arrived through, as far as the host can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedSignal {
    /// The host refused the call before the contract's own check ran.
    Trap,
    /// The contract reported its own error through a typed contract error.
    Panic,
}

impl ObservedSignal {
    /// Classifies a refusal.
    ///
    /// A refusal that carried a contract error code is the contract's own
    /// decision; a refusal without one is a bare trap, which is what a `panic!()`
    /// and a missing method both produce. That ambiguity is why the interface must
    /// be inspected before a refusal is read at all.
    #[must_use]
    pub const fn of(call: &ObservedCall) -> Option<Self> {
        match &call.outcome {
            CallResult::Refused { code: Some(_) } => Some(Self::Panic),
            CallResult::Refused { code: None } => Some(Self::Trap),
            CallResult::Returned | CallResult::CouldNotRun { .. } => None,
        }
    }

    /// Whether a profile's permitted signals include this one.
    ///
    /// `Either` is the common case and is not a relaxation: whether an absent
    /// authorization payload is refused by the host or by the contract's own check
    /// depends on where the implementation put the check, and both are correct.
    #[must_use]
    pub const fn permitted_by(self, allowed: FailureSignal) -> bool {
        match allowed {
            FailureSignal::Either => true,
            FailureSignal::Trap => matches!(self, Self::Trap),
            FailureSignal::Panic => matches!(self, Self::Panic),
            // A host error is what the environment produces before the contract is
            // reached. It is reported as an observation failure rather than as a
            // refusal, so it can never satisfy a requirement that asks for one.
            FailureSignal::HostError => false,
        }
    }
}

/// Evaluates the failure dimension.
///
/// # Errors
///
/// Returns an execution error when the state the requirement says must be
/// unchanged could not be read, because an unread state is not an unchanged one.
pub fn evaluate(
    profile: &ProfileBundle,
    vector: &VectorDocument,
    call: &ObservedCall,
    before: &dyn World,
    after: &dyn World,
) -> Result<Vec<AssertionOutcome>> {
    if vector.expected.outcome != ExpectedResult::Failure {
        return Ok(Vec::new());
    }

    let mut outcomes = Vec::new();

    // The first check is the one that catches `UNEXPECTED_SUCCESS`: a contract
    // that returns where the profile requires a refusal is not merely failing a
    // detail, it is violating the requirement the vector exists for.
    if let CallResult::CouldNotRun { detail } = &call.outcome {
        return Err(Error::new(
            ErrorClass::ExecutionError,
            format!(
                "vector `{}` requires a refusal, but the call could not be evaluated: {detail}",
                vector.id
            ),
        ));
    }

    outcomes.push(refusal_outcome(vector, call));

    if !call.refused() {
        // Nothing further is knowable: there was no refusal to describe. Returning
        // here rather than fabricating signal and state checks keeps the report
        // from claiming the contract's state was examined when it was not.
        return Ok(outcomes);
    }

    let definition = resolve(profile, vector)?;
    outcomes.push(signal_outcome(definition, vector, call));
    outcomes.extend(state_outcomes(profile, vector, definition, before, after)?);
    outcomes.push(events_outcome(definition, vector, call));

    Ok(outcomes)
}

/// Whether the call was refused at all, and whether that refusal is readable.
fn refusal_outcome(vector: &VectorDocument, call: &ObservedCall) -> AssertionOutcome {
    let detail = if call.refused() && !call.interface_verified {
        "The call aborted, but the interface was not inspected first, so the abort is not \
         distinguishible from a call to a method that does not exist. The requirement was not \
         established."
            .to_owned()
    } else if vector.expected.failure.is_none() && vector.expected.failure_category.is_none() {
        "The vector requires a refusal but names neither a failure requirement nor a failure \
         category, so the reason for the refusal is unconstrained."
            .to_owned()
    } else {
        format!(
            "The profile requires this path to be refused for a stated reason{}.",
            match &vector.expected.failure {
                Some(id) => format!(" (`{id}`)"),
                None => String::new(),
            }
        )
    };

    AssertionOutcome::from_boolean(
        format!("failure/outcome/{}", vector.id),
        AssertionCategory::Failure,
        "the call is refused",
        describe(call),
        call.refusal_is_unambiguous(),
    )
    .with_detail(detail)
}

/// Whether the refusal arrived through a signal the profile permits.
fn signal_outcome(
    definition: Option<&FailureDefinition>,
    vector: &VectorDocument,
    call: &ObservedCall,
) -> AssertionOutcome {
    let Some(definition) = definition else {
        // A profile-independent vector names a category rather than a definition,
        // and the profile it is executed against need not declare a matching one.
        // The category vocabulary is Estamora-level, so the check reduces to
        // whether the refusal was readable — already established — and the report
        // says so rather than inventing a signal requirement.
        return AssertionOutcome::passed(
            format!("failure/signal/{}", vector.id),
            AssertionCategory::Failure,
            "any signal the host produces",
            describe(call),
        )
        .with_detail(
            "The vector names a failure category rather than a profile failure definition, so \
             no signal requirement applies; the category itself is profile-level vocabulary."
                .to_owned(),
        );
    };

    let allowed = definition.expected.signal;
    let observed = ObservedSignal::of(call);
    let held = observed.is_some_and(|signal| signal.permitted_by(allowed));
    AssertionOutcome::from_boolean(
        format!("failure/signal/{}", definition.id),
        AssertionCategory::Failure,
        format!("signalled by {}", allowed.as_str()),
        observed.map_or_else(|| "not refused".to_owned(), describe_signal),
        held,
    )
    .with_detail(format!(
        "The payload is implementation-defined, so only the semantic category is required: {}",
        definition.category.as_str()
    ))
}

/// Whether the refused call left what it found.
fn state_outcomes(
    profile: &ProfileBundle,
    vector: &VectorDocument,
    definition: Option<&FailureDefinition>,
    before: &dyn World,
    after: &dyn World,
) -> Result<Vec<AssertionOutcome>> {
    let effect = definition.map_or(StateEffect::Reverted, |definition| {
        definition.expected.state_effect
    });
    if effect == StateEffect::Unspecified {
        return Ok(vec![AssertionOutcome::passed(
            format!("failure/state/{}", vector.id),
            AssertionCategory::Failure,
            "no state effect is required",
            "not examined",
        )
        .with_detail(
            "The profile is silent on the state a refusal must leave, so nothing is required of \
             the contract here."
                .to_owned(),
        )]);
    }

    let pairs = state::unchanged_pairs(profile, vector, before, after)?;
    Ok(pairs
        .into_iter()
        .enumerate()
        .map(|(position, pair)| {
            AssertionOutcome::from_boolean(
                format!("failure/state/{}/{position}", vector.id),
                AssertionCategory::Failure,
                pair.before.render(),
                pair.after.render(),
                pair.unchanged(),
            )
            .with_detail(format!(
                "{} must survive the refusal ({})",
                pair.subject,
                match effect {
                    StateEffect::Reverted => "reverted",
                    StateEffect::Unchanged => "unchanged",
                    StateEffect::Unspecified => "unspecified",
                }
            ))
        })
        .collect())
}

/// Whether the refusal emitted what the profile says it may.
fn events_outcome(
    definition: Option<&FailureDefinition>,
    vector: &VectorDocument,
    call: &ObservedCall,
) -> AssertionOutcome {
    let emitted = definition.map_or(EventsEmitted::None, |definition| {
        definition.expected.events_emitted
    });

    match emitted {
        EventsEmitted::Unspecified => AssertionOutcome::passed(
            format!("failure/events/{}", vector.id),
            AssertionCategory::Failure,
            "no event requirement is stated",
            format!("{} event(s) observed", call.events.len()),
        )
        .with_detail(
            "The profile is silent on what a refusal may emit, so nothing is required here."
                .to_owned(),
        ),
        EventsEmitted::None => AssertionOutcome::from_boolean(
            format!("failure/events/{}", vector.id),
            AssertionCategory::Failure,
            "no event is emitted",
            format!("{} event(s) observed", call.events.len()),
            call.events.is_empty(),
        )
        .with_detail(
            "A refusal that emitted the success event would corrupt every indexer built on the \
             standard, which is the failure this requirement exists to catch."
                .to_owned(),
        ),
    }
}

/// The profile's failure definition for this vector, where it names one.
fn resolve<'a>(
    profile: &'a ProfileBundle,
    vector: &VectorDocument,
) -> Result<Option<&'a FailureDefinition>> {
    match &vector.expected.failure {
        Some(id) => match profile.documents().failure(id) {
            Some(definition) => Ok(Some(definition)),
            None => Err(Error::new(
                ErrorClass::VectorError,
                format!(
                    "vector `{}` expects failure `{id}`, which the profile does not declare",
                    vector.id
                ),
            )),
        },
        None => Ok(None),
    }
}

/// A rendering of what was observed about the call.
fn describe(call: &ObservedCall) -> String {
    match &call.outcome {
        CallResult::Returned => "accepted".to_owned(),
        CallResult::Refused { code } => match code {
            Some(code) => format!("refused with contract error code {code}"),
            None => "refused with no contract error code".to_owned(),
        },
        CallResult::CouldNotRun { detail } => format!("could not run: {detail}"),
    }
}

/// A rendering of a signal.
fn describe_signal(signal: ObservedSignal) -> String {
    match signal {
        ObservedSignal::Trap => "trapped".to_owned(),
        ObservedSignal::Panic => "contract error".to_owned(),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::ObservedSignal;
    use estamora_profile::failures::FailureSignal;

    #[test]
    fn either_permits_both_signals_sep_41_implementations_produce() {
        assert!(ObservedSignal::Trap.permitted_by(FailureSignal::Either));
        assert!(ObservedSignal::Panic.permitted_by(FailureSignal::Either));
    }

    #[test]
    fn a_host_error_can_never_satisfy_a_requirement_for_a_refusal() {
        // A host error means the contract's own check was never reached, so it is
        // not evidence about the contract's behaviour.
        assert!(!ObservedSignal::Trap.permitted_by(FailureSignal::HostError));
        assert!(!ObservedSignal::Panic.permitted_by(FailureSignal::HostError));
    }

    #[test]
    fn a_specific_signal_only_accepts_itself() {
        assert!(ObservedSignal::Trap.permitted_by(FailureSignal::Trap));
        assert!(!ObservedSignal::Trap.permitted_by(FailureSignal::Panic));
        assert!(ObservedSignal::Panic.permitted_by(FailureSignal::Panic));
        assert!(!ObservedSignal::Panic.permitted_by(FailureSignal::Trap));
    }
}
