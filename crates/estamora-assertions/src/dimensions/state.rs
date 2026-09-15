//! The state dimension.
//!
//! A profile can only assert about state it can read through a method it declares.
//! That is a deliberate boundary rather than a limitation of the runner: a contract
//! whose invariants are untrue in storage the interface does not expose is a
//! contract Estamora cannot measure, and saying so is better than guessing at
//! storage keys the specification never named.
//!
//! # Why the resource is pinned
//!
//! A state assertion names the piece of state it reads. Without that pin, a
//! predicate could be evaluated against the wrong account while reading as though
//! it were about the right one — the right answer for the wrong subject. The
//! resource is what makes an assertion checkable by a reader who did not write it.
//!
//! # The whole-state snapshot
//!
//! Making a failure's state effect checkable needs something the per-assertion
//! checks do not provide: a comparison of *everything the vector declares* before
//! and after. That snapshot lives here rather than in the failure dimension because
//! it is a statement about state, and because the set of things it can see is
//! exactly the set the vector declared.

use estamora_core::{Error, ErrorClass, Result};
use estamora_profile::ProfileBundle;
use estamora_vectors::{AssertionCategory, StateAssertion, StateResourceRef, VectorDocument};

use crate::eval::{Environment, evaluate_predicate};
use crate::outcome::AssertionOutcome;
use crate::value::Value;
use crate::world::World;

/// The profile method used to read a balance.
///
/// Resolved from the profile rather than assumed, so that a profile which names
/// its balance accessor differently is still measurable, and so that the runner
/// never invents a method the specification did not declare.
const BALANCE_METHOD_ID: &str = "balance";

/// The profile method used to read an allowance.
const ALLOWANCE_METHOD_ID: &str = "allowance";

/// Evaluates every state assertion the vector declares.
///
/// # Errors
///
/// Returns an error when a predicate could not be evaluated: a declared read
/// failed, arithmetic left the representable range, or a value is not comparable
/// to the one the requirement names. None of those is a statement about the
/// contract.
pub fn evaluate(
    vector: &VectorDocument,
    environment: &Environment<'_>,
) -> Result<Vec<AssertionOutcome>> {
    let mut outcomes = Vec::new();
    for assertion in &vector.expected.state_assertions {
        outcomes.push(assertion_outcome(assertion, environment)?);
    }
    Ok(outcomes)
}

/// One reported state check.
fn assertion_outcome(
    assertion: &StateAssertion,
    environment: &Environment<'_>,
) -> Result<AssertionOutcome> {
    let evaluation = evaluate_predicate(environment, &assertion.predicate)?;
    let mut outcome = AssertionOutcome::from_evaluation(
        format!("state/{}", assertion.id),
        AssertionCategory::State,
        &evaluation,
    );
    let mut detail = format!("reads {}", describe_resource(&assertion.resource));
    if let Some(description) = &assertion.description {
        detail.push_str("; ");
        detail.push_str(description);
    }
    outcome = outcome.with_detail(detail);
    Ok(outcome)
}

/// A rendering of the piece of state an assertion reads.
fn describe_resource(resource: &StateResourceRef) -> String {
    match resource {
        StateResourceRef::Balance { account } => format!("the balance of `{account}`"),
        StateResourceRef::Allowance { from, spender } => {
            format!("the allowance `{from}` granted `{spender}`")
        },
        StateResourceRef::TotalSupply => "the total supply".to_owned(),
        StateResourceRef::LedgerSequence => "the ledger sequence".to_owned(),
        StateResourceRef::Custom { name, .. } => format!("the resource `{name}`"),
    }
}

/// One declared resource, read before and after the operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnchangedPair {
    /// What the pair is about, rendered for a reader.
    pub subject: String,
    /// The value before the operation.
    pub before: Value,
    /// The value after it.
    pub after: Value,
    /// The method it was read through.
    pub accessor: String,
}

impl UnchangedPair {
    /// Whether the resource was left as it was found.
    #[must_use]
    pub fn unchanged(&self) -> bool {
        self.before.equals(&self.after)
    }
}

/// Reads every balance and allowance the vector declares, before and after.
///
/// This is what makes `state_effect: reverted` and `state_effect: unchanged`
/// checkable. A contract can refuse a call correctly and still have written to
/// storage before its check ran, and the refusal alone cannot distinguish that from
/// a clean one — only a comparison can.
///
/// The pairs are returned rather than reported so that the state dimension and the
/// invariant dimension can each report them under their own category, which is what
/// keeps a passing state check from ever standing in for an invariant.
///
/// # Errors
///
/// Returns an error when the profile declares no balance accessor, because the
/// requirement would then be one the runner cannot apply. That is a profile or
/// corpus defect rather than a contract finding, and it is reported as such.
pub fn unchanged_pairs(
    profile: &ProfileBundle,
    vector: &VectorDocument,
    before: &dyn World,
    after: &dyn World,
) -> Result<Vec<UnchangedPair>> {
    let accessor = balance_accessor(profile).ok_or_else(|| {
        Error::new(
            ErrorClass::ProfileError,
            format!(
                "profile `{}` declares no `{BALANCE_METHOD_ID}` method, so a requirement about \
                 unchanged state cannot be evaluated",
                profile.document().profile.id
            ),
        )
    })?;

    let mut pairs = Vec::new();
    for actor in vector.fixtures.balances.keys() {
        let arguments = [Value::Address(actor.clone())];
        pairs.push(UnchangedPair {
            subject: format!("the balance of `{actor}`"),
            before: before.read(&accessor, &arguments)?,
            after: after.read(&accessor, &arguments)?,
            accessor: accessor.clone(),
        });
    }

    if let Some(accessor) = allowance_accessor(profile) {
        for allowance in &vector.fixtures.allowances {
            let arguments = [
                Value::Address(allowance.from.clone()),
                Value::Address(allowance.spender.clone()),
            ];
            pairs.push(UnchangedPair {
                subject: format!(
                    "the allowance `{}` granted `{}`",
                    allowance.from, allowance.spender
                ),
                before: before.read(&accessor, &arguments)?,
                after: after.read(&accessor, &arguments)?,
                accessor: accessor.clone(),
            });
        }
    }

    Ok(pairs)
}

/// The per-resource checks the state dimension reports.
///
/// # Errors
///
/// As [`unchanged_pairs`].
pub fn snapshot_outcomes(
    profile: &ProfileBundle,
    vector: &VectorDocument,
    before: &dyn World,
    after: &dyn World,
) -> Result<Vec<AssertionOutcome>> {
    let pairs = unchanged_pairs(profile, vector, before, after)?;
    let mut outcomes = Vec::new();
    for (position, pair) in pairs.iter().enumerate() {
        outcomes.push(
            AssertionOutcome::from_boolean(
                format!("state/unchanged/{}/{position}", vector.id),
                AssertionCategory::State,
                pair.before.render(),
                pair.after.render(),
                pair.unchanged(),
            )
            .with_detail(format!(
                "{} must be unchanged, read through `{}`",
                pair.subject, pair.accessor
            )),
        );
    }
    Ok(outcomes)
}

/// The method the profile declares for reading a balance.
fn balance_accessor(profile: &ProfileBundle) -> Option<String> {
    accessor(profile, BALANCE_METHOD_ID)
}

/// The method the profile declares for reading an allowance.
fn allowance_accessor(profile: &ProfileBundle) -> Option<String> {
    accessor(profile, ALLOWANCE_METHOD_ID)
}

/// Resolves a profile method by id, preferring the id and falling back to the name.
fn accessor(profile: &ProfileBundle, id: &str) -> Option<String> {
    let methods = &profile.methods().methods;
    methods
        .iter()
        .find(|method| {
            method.id == id && method.mutability == estamora_profile::types::Mutability::Readonly
        })
        .or_else(|| {
            methods.iter().find(|method| {
                method.name == id
                    && method.mutability == estamora_profile::types::Mutability::Readonly
            })
        })
        .map(|method| method.name.clone())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::describe_resource;
    use estamora_vectors::StateResourceRef;

    #[test]
    fn a_resource_is_rendered_by_what_it_reads() {
        assert_eq!(
            describe_resource(&StateResourceRef::Balance {
                account: "alice".to_owned()
            }),
            "the balance of `alice`"
        );
        assert_eq!(
            describe_resource(&StateResourceRef::TotalSupply),
            "the total supply"
        );
    }
}
