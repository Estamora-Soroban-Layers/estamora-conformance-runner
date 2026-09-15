//! The behavioural dimension.
//!
//! This is where a profile stops describing shape and starts describing conduct.
//! Each rule is one operation's contract with its caller: the preconditions under
//! which it applies, the postconditions that must hold after it, the failures it is
//! allowed to produce, the events it must emit, and the invariants that must survive
//! it.
//!
//! # A precondition that does not hold makes a rule inapplicable
//!
//! A precondition is not an assertion about the contract; it is a statement that
//! *this scenario is the one the rule describes*. If a vector were fed a rule whose
//! precondition did not hold, the postconditions would be evaluated against a world
//! the rule never claimed anything about, and a passing result would mean nothing.
//! So an unheld precondition records the rule as inapplicable and reports no result
//! for it, rather than reporting a failure the contract did not cause or a pass it
//! did not earn.
//!
//! # Success and failure rules are separate for a reason
//!
//! A profile that only described successful behaviour would accept a contract that
//! silently succeeds where the standard requires a rejection. That is the most
//! dangerous kind of non-conformance, because every happy-path test misses it.
//!
//! # The one corpus-consistency check
//!
//! When a rule is a failure rule and the vector names a failure, the vector's
//! failure must be one the rule expects. A vector that constructs a scenario the
//! rule does not describe would otherwise report a pass for a requirement it never
//! exercised, and that is a defect in the corpus rather than in the contract — which
//! is exactly the kind of thing this check is for.

use estamora_core::Result;
use estamora_core::expr::Predicate;
use estamora_profile::ProfileBundle;
use estamora_profile::behavior::{BehaviorKind, BehaviorRule};
use estamora_vectors::{AssertionCategory, ExpectedResult, VectorDocument};

use crate::eval::{Environment, evaluate_predicate};
use crate::observation::ObservedCall;
use crate::outcome::AssertionOutcome;

use super::events;

/// What the behavioural dimension concluded for one vector.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BehaviorReport {
    /// The checks that were reported.
    pub assertions: Vec<AssertionOutcome>,
    /// Rules that did not apply, with the reason.
    ///
    /// Recorded rather than dropped, because a vector that exercises no rule at all
    /// is a corpus defect worth seeing, and a set of silent skips would look
    /// identical to a set of passes in a summary line.
    pub inapplicable: Vec<(String, String)>,
    /// Identifiers of the invariants the applicable rules require.
    pub invariants: Vec<String>,
}

/// Evaluates every behavioural rule that governs the vector's method.
///
/// # Errors
///
/// Returns an error when a precondition or postcondition could not be evaluated.
/// A comparison the runner could not make is not a failed requirement.
pub fn evaluate(
    profile: &ProfileBundle,
    vector: &VectorDocument,
    before: &Environment<'_>,
    environment: &Environment<'_>,
    call: &ObservedCall,
) -> Result<BehaviorReport> {
    let method = method_id(profile, vector);
    let rules = profile.documents().behaviors_for(&method);

    let mut report = BehaviorReport::default();
    for rule in rules {
        evaluate_rule(
            rule,
            profile,
            vector,
            before,
            environment,
            call,
            &mut report,
        )?;
    }
    Ok(report)
}

/// Evaluates one rule.
#[allow(
    clippy::too_many_arguments,
    reason = "each argument is a distinct observation the rule needs"
)]
fn evaluate_rule(
    rule: &BehaviorRule,
    profile: &ProfileBundle,
    vector: &VectorDocument,
    before: &Environment<'_>,
    environment: &Environment<'_>,
    call: &ObservedCall,
    report: &mut BehaviorReport,
) -> Result<()> {
    if let Some(reason) = inapplicability(rule, vector) {
        report.inapplicable.push((rule.id.clone(), reason));
        return Ok(());
    }

    // Preconditions are evaluated against the world the operation started in: they
    // describe the scenario, so they cannot depend on its result.
    for precondition in &rule.preconditions {
        let evaluation = evaluate_predicate(before, precondition)?;
        if !evaluation.held {
            report.inapplicable.push((
                rule.id.clone(),
                format!(
                    "precondition `{}` did not hold: expected {}, observed {}",
                    precondition.kind(),
                    evaluation.expected,
                    evaluation.observed
                ),
            ));
            return Ok(());
        }
    }

    for invariant in &rule.invariants {
        if !report.invariants.contains(invariant) {
            report.invariants.push(invariant.clone());
        }
    }

    let id = format!("behavior/{}", rule.id);

    report.assertions.push(outcome_check(rule, vector, call));

    for (position, postcondition) in rule.postconditions.iter().enumerate() {
        report.assertions.push(postcondition_outcome(
            rule,
            environment,
            postcondition,
            position,
        )?);
    }

    if !rule.expect_failures.is_empty() {
        report
            .assertions
            .push(expected_failure_outcome(rule, vector, call));
    }

    for event in &rule.expect_events {
        report
            .assertions
            .push(events::required_by_rule(profile, event, &id, call));
    }

    for event in &rule.forbid_events {
        report
            .assertions
            .push(events::forbidden_by_rule(profile, event, &id, call));
    }

    Ok(())
}

/// Why a rule does not describe the scenario a vector constructs.
///
/// Applicability is decided in two steps, and both are declarative rather than
/// guessed:
///
/// 1. **The outcome.** A rule that describes a path that must *fail* cannot be a
///    claim about a vector that expects success, and the reverse. A profile's rules
///    partition by that field, because there is no other fact available to the
///    runner at this point: whether authorization was supplied is a property of the
///    scenario, not of the contract's state.
/// 2. **The classification.** When a failure rule names the failures it expects and
///    the vector names one too, the vector's must be among them. A vector that
///    constructs an insufficient-balance scenario is not evidence about a rule that
///    exists to catch a missing authorization.
///
/// A rule that does not apply is recorded as inapplicable rather than satisfied.
/// Counting it as a pass would claim coverage the scenario never exercised, which is
/// the same defect as silently skipping it.
fn inapplicability(rule: &BehaviorRule, vector: &VectorDocument) -> Option<String> {
    let expects_failure = vector.expected.outcome == ExpectedResult::Failure;
    match rule.kind {
        BehaviorKind::Success if expects_failure => {
            return Some(
                "the rule describes a path that must succeed and the vector expects failure"
                    .to_string(),
            );
        },
        BehaviorKind::Failure if !expects_failure => {
            return Some(
                "the rule describes a path that must fail and the vector expects success"
                    .to_string(),
            );
        },
        BehaviorKind::Success | BehaviorKind::Failure => {},
    }

    if rule.kind == BehaviorKind::Failure
        && !rule.expect_failures.is_empty()
        && let Some(named) = vector.expected.failure.as_deref()
        && !rule
            .expect_failures
            .iter()
            .any(|expected| expected == named)
    {
        return Some(format!(
            "the vector constructs `{named}`, which this rule does not describe"
        ));
    }

    None
}

/// Whether the operation ended the way the rule says it must.
fn outcome_check(
    rule: &BehaviorRule,
    vector: &VectorDocument,
    call: &ObservedCall,
) -> AssertionOutcome {
    let expected = match rule.kind {
        BehaviorKind::Success => "accepted",
        BehaviorKind::Failure => "refused",
    };
    let held = match rule.kind {
        BehaviorKind::Success => call.accepted(),
        // A rule that requires a failure is only satisfied by a readable refusal:
        // an abort with an uninspected interface could be a missing method.
        BehaviorKind::Failure => call.refusal_is_unambiguous(),
    };
    AssertionOutcome::from_boolean(
        format!("behavior/{}/outcome/{}", rule.id, vector.id),
        AssertionCategory::Behavior,
        expected,
        describe(call),
        held,
    )
    .with_detail(rule.summary.clone())
}

/// Whether one postcondition holds.
fn postcondition_outcome(
    rule: &BehaviorRule,
    environment: &Environment<'_>,
    postcondition: &Predicate,
    position: usize,
) -> Result<AssertionOutcome> {
    let evaluation = evaluate_predicate(environment, postcondition)?;
    Ok(AssertionOutcome::from_evaluation(
        format!("behavior/{}/postcondition/{position}", rule.id),
        AssertionCategory::Behavior,
        &evaluation,
    )
    .with_detail(format!("{} ({})", rule.summary, postcondition.kind())))
}

/// Whether the vector constructs the failure the rule expects.
fn expected_failure_outcome(
    rule: &BehaviorRule,
    vector: &VectorDocument,
    call: &ObservedCall,
) -> AssertionOutcome {
    let named = vector.expected.failure.as_deref();
    // Applicability has already established that the vector constructs a scenario
    // this rule describes. What remains to check is the part that cannot be decided
    // from the documents: that a *readable* refusal actually happened, so that the
    // rule's expected failure was reached rather than merely described.
    let held = call.refusal_is_unambiguous()
        && match named {
            Some(failure) => rule
                .expect_failures
                .iter()
                .any(|expected| expected == failure),
            None => vector.expected.outcome == ExpectedResult::Failure,
        };
    AssertionOutcome::from_boolean(
        format!("behavior/{}/failure", rule.id),
        AssertionCategory::Behavior,
        format!(
            "a refusal classified as one of [{}]",
            rule.expect_failures.join(", ")
        ),
        named.map_or_else(
            || format!("a refusal of no named classification ({})", describe(call)),
            ToOwned::to_owned,
        ),
        held,
    )
    .with_detail(
        "Checked so that a vector cannot report a pass for a rule whose scenario it did not \
         construct. The refusal's own category and state effect are reported by the failure \
         dimension."
            .to_owned(),
    )
}

/// The method id a vector exercises, which is what a rule's `method` names.
fn method_id(profile: &ProfileBundle, vector: &VectorDocument) -> String {
    profile
        .methods()
        .methods
        .iter()
        .find(|method| method.name == vector.method)
        .map_or_else(|| vector.method.clone(), |method| method.id.clone())
}

/// A rendering of what was observed about the call.
fn describe(call: &ObservedCall) -> String {
    match &call.outcome {
        crate::observation::CallResult::Returned => "accepted".to_owned(),
        crate::observation::CallResult::Refused { code } => match code {
            Some(code) => format!("refused with contract error code {code}"),
            None => "refused".to_owned(),
        },
        crate::observation::CallResult::CouldNotRun { detail } => {
            format!("could not run: {detail}")
        },
    }
}
