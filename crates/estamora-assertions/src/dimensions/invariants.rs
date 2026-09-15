//! The invariant dimension.
//!
//! An invariant is a property that must survive an operation, stated once and
//! reused across vectors. It is deliberately not a comment: "balances are
//! conserved" is a claim the runner evaluates, and a claim it can fail.
//!
//! # Six families, because the family decides what has to be observed
//!
//! Conservation needs an aggregate before and after; `state_unchanged` needs every
//! declared resource before and after; `authorization_blocks_mutation` needs the
//! same, but is only meaningful for a call that was refused; `monotonic` needs the
//! direction an aggregate moved; `bounds` needs each member of a set against a
//! predicate; `predicate` needs whatever the predicate names. Treating them as one
//! kind would force the runner to guess which observation a rule was about, and a
//! guess is how a requirement becomes unenforced without anyone noticing.
//!
//! # Scope is a filter, not a suggestion
//!
//! Conservation is true of a transfer and false of a mint, so an unscoped invariant
//! would be wrong for at least one operation in the profile. An invariant outside
//! its scope is therefore **inapplicable** for a vector rather than satisfied by it:
//! counting it as a pass would make a run report coverage it does not have.
//!
//! # A warning is recorded, never promoted
//!
//! A property an upstream document states as SHOULD rather than MUST is declared
//! `warning`. A violated warning must not turn a passing run into a failing one,
//! and it must not be reported as a passed assertion either. It is recorded as a
//! diagnostic — visible in the report, excluded from the verdict — which is the only
//! reading that keeps the runner from inventing a stricter standard than the one it
//! claims to encode.

use std::collections::BTreeSet;

use estamora_core::{Error, ErrorClass, Result};
use estamora_profile::ProfileBundle;
use estamora_profile::invariants::{
    InvariantDefinition, InvariantKind, InvariantOutcome, InvariantSeverity, MonotonicDirection,
};
use estamora_vectors::{AssertionCategory, VectorDocument};

use crate::eval::{Environment, evaluate_predicate, evaluate_predicate_for_member};
use crate::observation::ObservedCall;
use crate::outcome::{AssertionOutcome, OutcomeDiagnostic};
use crate::value::Value;
use crate::world::World;

use super::state;

/// What the invariant dimension concluded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InvariantReport {
    /// The checks that were reported.
    pub assertions: Vec<AssertionOutcome>,
    /// Violated warnings, which are recorded and do not decide the run.
    pub warnings: Vec<OutcomeDiagnostic>,
}

/// Evaluates every invariant that is in scope for this call.
///
/// # Errors
///
/// Returns an error when a required value could not be read or compared. A read a
/// contract cannot answer is not a violated invariant: nothing was measured.
pub fn evaluate(
    profile: &ProfileBundle,
    vector: &VectorDocument,
    selected: &[String],
    environment: &Environment<'_>,
    before: &dyn World,
    after: &dyn World,
    call: &ObservedCall,
) -> Result<InvariantReport> {
    let outcome = if call.accepted() {
        InvariantOutcome::Success
    } else {
        InvariantOutcome::Failure
    };

    let mut report = InvariantReport::default();

    // The profile is normative, so its scoped invariants apply whether or not the
    // vector names them; the vector's own list selects additional ones. Deduping by
    // identifier keeps a rule named in both places from being reported twice.
    let scoped = profile
        .documents()
        .invariants_for(&method_id(profile, vector), outcome);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut chosen: Vec<&InvariantDefinition> = Vec::new();

    for invariant in scoped {
        seen.insert(invariant.id.clone());
        chosen.push(invariant);
    }

    for id in selected {
        if seen.contains(id) {
            continue;
        }
        let Some(invariant) = profile.documents().invariant(id) else {
            return Err(Error::new(
                ErrorClass::VectorError,
                format!(
                    "vector `{}` names invariant `{id}`, which the profile does not declare",
                    vector.id
                ),
            ));
        };
        chosen.push(invariant);
    }

    for invariant in chosen {
        evaluate_one(
            profile,
            invariant,
            vector,
            environment,
            before,
            after,
            call,
            &mut report,
        )?;
    }

    Ok(report)
}

/// The method id a vector exercises, which is what an invariant's scope names.
fn method_id(profile: &ProfileBundle, vector: &VectorDocument) -> String {
    profile
        .methods()
        .methods
        .iter()
        .find(|method| method.name == vector.method)
        .map_or_else(|| vector.method.clone(), |method| method.id.clone())
}

/// Evaluates one invariant and records what it concluded.
#[allow(
    clippy::too_many_arguments,
    reason = "every argument is one distinct observation the six families need"
)]
fn evaluate_one(
    profile: &ProfileBundle,
    invariant: &InvariantDefinition,
    vector: &VectorDocument,
    environment: &Environment<'_>,
    before: &dyn World,
    after: &dyn World,
    call: &ObservedCall,
    report: &mut InvariantReport,
) -> Result<()> {
    let mut outcomes = match invariant.kind {
        InvariantKind::Conservation => conservation(invariant, before, after)?,
        InvariantKind::Monotonic => monotonic(invariant, before, after)?,
        InvariantKind::Bounds => bounds(invariant, environment)?,
        InvariantKind::Predicate => predicate(invariant, environment)?,
        InvariantKind::StateUnchanged => unchanged(profile, invariant, vector, before, after)?,
        InvariantKind::AuthorizationBlocksMutation => {
            // Only meaningful for a call the contract refused. For an accepted call
            // the invariant says nothing, and reporting it as satisfied would claim
            // coverage the scenario never exercised.
            if call.accepted() {
                Vec::new()
            } else {
                unchanged(profile, invariant, vector, before, after)?
            }
        },
    };

    for outcome in &mut outcomes {
        if let Some(description) = &invariant.description {
            outcome.detail = Some(match outcome.detail.take() {
                Some(existing) => format!("{existing}; {description}"),
                None => description.clone(),
            });
        }
    }

    for outcome in outcomes {
        let violated = outcome.status == estamora_core::AssertionStatus::Failed;
        if violated && invariant.severity == InvariantSeverity::Warning {
            report.warnings.push(OutcomeDiagnostic::new(
                format!("invariant-warning.{}", invariant.id),
                format!(
                    "{}: {} — expected {}, observed {}",
                    invariant.summary, invariant.title, outcome.expected, outcome.observed
                ),
            ));
            continue;
        }
        report.assertions.push(outcome);
    }

    Ok(())
}

/// An aggregate over a resource set must not have moved.
fn conservation(
    invariant: &InvariantDefinition,
    before: &dyn World,
    after: &dyn World,
) -> Result<Vec<AssertionOutcome>> {
    let resource = required_resource(invariant)?;
    let started = aggregate(before, resource)?;
    let ended = aggregate(after, resource)?;
    Ok(vec![
        AssertionOutcome::from_boolean(
            format!("invariant/{}/{}", invariant.kind.as_str(), invariant.id),
            AssertionCategory::Invariant,
            format!("{} over `{resource}`", started.render()),
            ended.render(),
            started.equals(&ended),
        )
        .with_detail(invariant.summary.clone()),
    ])
}

/// An aggregate may move in one direction and no other.
fn monotonic(
    invariant: &InvariantDefinition,
    before: &dyn World,
    after: &dyn World,
) -> Result<Vec<AssertionOutcome>> {
    let resource = required_resource(invariant)?;
    let direction = invariant.direction.ok_or_else(|| {
        Error::new(
            ErrorClass::ProfileError,
            format!(
                "invariant `{}` is monotonic but declares no direction",
                invariant.id
            ),
        )
    })?;
    let started = aggregate(before, resource)?;
    let ended = aggregate(after, resource)?;

    let (Some(from), Some(to)) = (started.as_integer(), ended.as_integer()) else {
        return Err(Error::new(
            ErrorClass::ExecutionError,
            format!(
                "invariant `{}` asserts a direction of movement for `{resource}`, whose aggregate \
                 is not a quantity",
                invariant.id
            ),
        ));
    };

    let held = match direction {
        MonotonicDirection::NonDecreasing => to >= from,
        MonotonicDirection::NonIncreasing => to <= from,
    };

    Ok(vec![
        AssertionOutcome::from_boolean(
            format!("invariant/{}/{}", invariant.kind.as_str(), invariant.id),
            AssertionCategory::Invariant,
            format!(
                "{} over `{resource}`",
                match direction {
                    MonotonicDirection::NonDecreasing => "not falling",
                    MonotonicDirection::NonIncreasing => "not rising",
                }
            ),
            format!("{} → {}", started.render(), ended.render()),
            held,
        )
        .with_detail(invariant.summary.clone()),
    ])
}

/// Every member of a resource set must satisfy the declared predicate.
fn bounds(
    invariant: &InvariantDefinition,
    environment: &Environment<'_>,
) -> Result<Vec<AssertionOutcome>> {
    let resource = required_resource(invariant)?;
    let predicate = invariant.predicate.as_ref().ok_or_else(|| {
        Error::new(
            ErrorClass::ProfileError,
            format!(
                "invariant `{}` is bounded but declares no predicate",
                invariant.id
            ),
        )
    })?;

    let members = environment.after.resource_members(resource)?;
    let mut outcomes = Vec::new();
    for (position, member) in members.iter().enumerate() {
        let evaluation = evaluate_predicate_for_member(environment, predicate, member)?;
        outcomes.push(
            AssertionOutcome::from_evaluation(
                format!(
                    "invariant/{}/{}/{position}",
                    invariant.kind.as_str(),
                    invariant.id
                ),
                AssertionCategory::Invariant,
                &evaluation,
            )
            .with_detail(format!(
                "member {position} of `{resource}`: {}",
                invariant.summary
            )),
        );
    }
    Ok(outcomes)
}

/// The declared predicate must hold.
fn predicate(
    invariant: &InvariantDefinition,
    environment: &Environment<'_>,
) -> Result<Vec<AssertionOutcome>> {
    let predicate = invariant.predicate.as_ref().ok_or_else(|| {
        Error::new(
            ErrorClass::ProfileError,
            format!("invariant `{}` declares no predicate", invariant.id),
        )
    })?;
    let evaluation = evaluate_predicate(environment, predicate)?;
    Ok(vec![
        AssertionOutcome::from_evaluation(
            format!("invariant/{}/{}", invariant.kind.as_str(), invariant.id),
            AssertionCategory::Invariant,
            &evaluation,
        )
        .with_detail(invariant.summary.clone()),
    ])
}

/// Nothing the vector declares may have changed.
fn unchanged(
    profile: &ProfileBundle,
    invariant: &InvariantDefinition,
    vector: &VectorDocument,
    before: &dyn World,
    after: &dyn World,
) -> Result<Vec<AssertionOutcome>> {
    let pairs = state::unchanged_pairs(profile, vector, before, after)?;

    Ok(pairs
        .into_iter()
        .enumerate()
        .map(|(position, pair)| {
            AssertionOutcome::from_boolean(
                format!(
                    "invariant/{}/{}/{position}",
                    invariant.kind.as_str(),
                    invariant.id
                ),
                AssertionCategory::Invariant,
                pair.before.render(),
                pair.after.render(),
                pair.unchanged(),
            )
            .with_detail(format!("{}: {}", invariant.summary, pair.subject))
        })
        .collect())
}

/// The resource an aggregate invariant ranges over.
fn required_resource(invariant: &InvariantDefinition) -> Result<&str> {
    invariant.resource.as_deref().ok_or_else(|| {
        Error::new(
            ErrorClass::ProfileError,
            format!(
                "invariant `{}` is a {} check but names no resource",
                invariant.id,
                invariant.kind.as_str()
            ),
        )
    })
}

/// The sum of every member of a resource set.
fn aggregate(world: &dyn World, resource: &str) -> Result<Value> {
    let members = world.resource_members(resource)?;
    let mut total: i128 = 0;
    for member in members {
        let Some(value) = member.as_integer() else {
            return Err(Error::new(
                ErrorClass::ExecutionError,
                format!(
                    "the resource set `{resource}` holds {}, which is not a quantity, so no \
                     aggregate can be taken",
                    member.render()
                ),
            ));
        };
        total = total.checked_add(value).ok_or_else(|| {
            Error::new(
                ErrorClass::ExecutionError,
                format!("summing the resource set `{resource}` left the representable range"),
            )
        })?;
    }
    Ok(Value::Integer(total))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::aggregate;
    use crate::value::Value;
    use crate::world::World;
    use estamora_core::Result;

    /// A world whose balances are fixed, so that an aggregate is a statement about
    /// arithmetic rather than about a contract.
    struct Fixed(Vec<Value>);

    impl World for Fixed {
        fn read(&self, _method: &str, _args: &[Value]) -> Result<Value> {
            Ok(Value::Absent)
        }
        fn resource_members(&self, _resource: &str) -> Result<Vec<Value>> {
            Ok(self.0.clone())
        }
        fn ledger_sequence(&self) -> u32 {
            0
        }
        fn allowance_expiry(&self, _from: &str, _spender: &str) -> Option<u32> {
            None
        }
    }

    #[test]
    fn an_aggregate_is_exact_over_many_members() {
        let world = Fixed(vec![
            Value::Integer(1_000),
            Value::Integer(250),
            Value::Integer(-250),
        ]);
        assert_eq!(
            aggregate(&world, "balances").unwrap(),
            Value::Integer(1_000)
        );
    }

    #[test]
    fn a_member_that_is_not_a_quantity_is_an_execution_failure() {
        // Not a violated invariant: the runner could not compute the aggregate at
        // all, so nothing about the contract follows from it.
        let world = Fixed(vec![Value::Integer(1), Value::Text("many".to_owned())]);
        let problem = aggregate(&world, "balances").unwrap_err();
        assert_eq!(
            problem.class(),
            estamora_core::ErrorClass::ExecutionError,
            "an incomputable aggregate must not be reported as a contract failure"
        );
    }
}
