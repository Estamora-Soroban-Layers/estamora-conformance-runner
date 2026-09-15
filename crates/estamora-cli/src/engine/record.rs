//! The world before the operation.
//!
//! # Why a world has to be recorded rather than merely read
//!
//! A requirement can say the sender's balance *fell* by the amount. That is a statement
//! about a change, so it needs both endpoints, and the first endpoint stops existing the
//! moment the operation runs. A world that read the contract lazily would therefore
//! compare the state after the call with the state after the call, every relative
//! requirement would hold vacuously, and the whole class of requirements that catch a
//! contract which moves the wrong amount would silently stop working.
//!
//! So every read a requirement might perform is worked out before the call, performed
//! once, and served from the record afterwards.
//!
//! # What is recorded
//!
//! Four sources, and the union is deliberate rather than any one of them:
//!
//! 1. **The fixture's own declaration.** Every actor's balance, every allowance the
//!    fixture granted, and the total supply where the profile exposes an accessor.
//!    This is what an aggregate over a resource set ranges across, and it is why the
//!    set is the one the vector declared rather than one the runner invented.
//! 2. **The vector's expectations.** Its state assertions, its event expectations and
//!    its assertions, each walked for the reads they name.
//! 3. **The profile's rules for this method**, so that a relative rule supplied by the
//!    profile rather than the vector is answerable too.
//! 4. **The profile's invariants**, which range over resource sets and can therefore
//!    name reads that appear nowhere in the vector.
//!
//! # A read that cannot be performed is recorded as such
//!
//! Not skipped. A read the runner could not make is a read the requirement that needed
//! it cannot be evaluated against, and the honest result is an undecidable vector. If
//! it were dropped the requirement would compare an absent value, and a contract would
//! be reported as violating something the runner never measured.

use std::collections::BTreeMap;

use estamora_core::expr::ValueExpr;
use estamora_core::value::Value;
use estamora_core::world::World;
use estamora_core::{Error, ErrorClass, Result};
use estamora_profile::ProfileBundle;
use estamora_soroban::{Actor, AllowanceGrant, ContractWorld};
use estamora_vectors::VectorDocument;

use crate::engine::values;

/// One recorded read.
#[derive(Debug, Clone)]
enum Recorded {
    /// The read was performed.
    Value(Value),
    /// The read could not be performed, with the reason.
    Unavailable(String),
}

/// The world as it was before the operation ran.
///
/// Serves only what was recorded, and refuses anything else with an execution
/// failure rather than an answer. An answer it invented would be indistinguishable
/// from a measurement, which is the one thing an observation must never be.
#[derive(Debug, Clone)]
pub struct RecordedWorld {
    ledger_sequence: u32,
    reads: Vec<((String, Vec<Value>), Recorded)>,
    expiries: Vec<((String, String), Option<u32>)>,
}

impl RecordedWorld {
    /// An empty record, for a vector that has no before-state to speak of.
    ///
    /// Used by tests that evaluate a rule against a single world and by the report
    /// rendering path, where the record is never consulted.
    #[must_use]
    pub const fn empty(ledger_sequence: u32) -> Self {
        Self {
            ledger_sequence,
            reads: Vec::new(),
            expiries: Vec::new(),
        }
    }

    /// How many reads were recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.reads.len()
    }

    /// Whether nothing was recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.reads.is_empty()
    }

    /// Records a read that succeeded.
    pub fn insert(&mut self, method: impl Into<String>, args: Vec<Value>, value: Value) {
        let key = (method.into(), args);
        if !self.reads.iter().any(|(existing, _)| *existing == key) {
            self.reads.push((key, Recorded::Value(value)));
        }
    }

    /// Records a read that could not be performed.
    pub fn insert_unavailable(
        &mut self,
        method: impl Into<String>,
        args: Vec<Value>,
        reason: impl Into<String>,
    ) {
        let key = (method.into(), args);
        if !self.reads.iter().any(|(existing, _)| *existing == key) {
            self.reads.push((key, Recorded::Unavailable(reason.into())));
        }
    }

    /// Records an allowance's expiry.
    pub fn insert_expiry(&mut self, from: &str, spender: &str, expiry: Option<u32>) {
        self.expiries
            .push(((from.to_owned(), spender.to_owned()), expiry));
    }
}

impl World for RecordedWorld {
    fn read(&self, method: &str, args: &[Value]) -> Result<Value> {
        let key = (method.to_owned(), args.to_vec());
        let Some((_, recorded)) = self.reads.iter().find(|(existing, _)| *existing == key) else {
            return Err(Error::new(
                ErrorClass::ExecutionError,
                format!(
                    "the state before the operation does not include `{method}`, so a requirement \
                     written against it cannot be evaluated; the runner records the reads a \
                     requirement names before the call, and this one was not among them"
                ),
            )
            .with_context("method", method));
        };
        match recorded {
            Recorded::Value(value) => Ok(value.clone()),
            Recorded::Unavailable(reason) => Err(Error::new(
                ErrorClass::ExecutionError,
                format!("the state before the operation could not be read: {reason}"),
            )
            .with_context("method", method)),
        }
    }

    fn resource_members(&self, resource: &str) -> Result<Vec<Value>> {
        let wanted = match resource {
            estamora_soroban::world::BALANCES => "balance",
            estamora_soroban::world::ALLOWANCES => "allowance",
            other => {
                return Err(Error::new(
                    ErrorClass::ExecutionError,
                    format!(
                        "the resource set `{other}` was not recorded before the operation; only \
                         the sets a vector declares can be read"
                    ),
                )
                .with_context("resource", other.to_owned()));
            },
        };
        // Insertion order is the fixture's declaration order, which is stable across
        // runs, so a run that names a member names the same one every time.
        let mut members = Vec::new();
        for ((method, _), recorded) in &self.reads {
            if method == wanted {
                match recorded {
                    Recorded::Value(value) => members.push(value.clone()),
                    Recorded::Unavailable(reason) => {
                        return Err(Error::new(
                            ErrorClass::ExecutionError,
                            format!(
                                "the resource set `{resource}` could not be read before the \
                                 operation: {reason}"
                            ),
                        ));
                    },
                }
            }
        }
        if members.is_empty() {
            return Err(Error::new(
                ErrorClass::ExecutionError,
                format!(
                    "the resource set `{resource}` has no recorded members, so a requirement \
                     over it has nothing to range across"
                ),
            ));
        }
        Ok(members)
    }

    fn ledger_sequence(&self) -> u32 {
        self.ledger_sequence
    }

    fn allowance_expiry(&self, from: &str, spender: &str) -> Option<u32> {
        self.expiries
            .iter()
            .find(|((holder, allowed), _)| holder == from && allowed == spender)
            .and_then(|(_, expiry)| *expiry)
    }
}

/// Records the state `world` reports, before the operation runs.
///
/// `profile` is consulted for the rules and invariants that apply, and `vector` for
/// the fixture and the expectations, so the record holds exactly what a requirement
/// could ask for and nothing that merely happens to be readable.
#[must_use]
pub fn capture(
    profile: &ProfileBundle,
    vector: &VectorDocument,
    world: &ContractWorld<'_>,
) -> RecordedWorld {
    let mut record = RecordedWorld::empty(vector.fixtures.ledger.sequence);

    // 1. The fixture's own declaration. The order is the fixture's declaration order
    //    so that an aggregate's members are the same in every run.
    for actor in &vector.fixtures.actors {
        let name = Value::Address(actor.name.clone());
        record_read(world, &mut record, "balance", vec![name]);
    }
    for allowance in &vector.fixtures.allowances {
        let args = vec![
            Value::Address(allowance.from.clone()),
            Value::Address(allowance.spender.clone()),
        ];
        record_read(world, &mut record, "allowance", args);
        record.insert_expiry(
            &allowance.from,
            &allowance.spender,
            world.allowance_expiry(&allowance.from, &allowance.spender),
        );
    }
    // The total supply, where the profile exposes an accessor for it. Asked for by
    // name because there is no other way to know whether a contract publishes one, and
    // a read of a method that is not there is recorded as unavailable rather than
    // guessed at.
    if profile
        .documents()
        .methods
        .methods
        .iter()
        .any(|method| method.name == "total_supply")
    {
        record_read(world, &mut record, "total_supply", Vec::new());
    }

    // 2. The vector's expectations.
    let mut expressions: Vec<ValueExpr> = Vec::new();
    for assertion in &vector.expected.state_assertions {
        collect_predicate(&assertion.predicate, &mut expressions);
    }
    for expectation in &vector.expected.events.required {
        expressions.extend(expectation.topics.iter().cloned());
        expressions.extend(expectation.data.iter().cloned());
    }
    for assertion in &vector.assertions {
        collect_predicate(&assertion.predicate, &mut expressions);
    }
    if let Some(returns) = &vector.expected.returns {
        expressions.push(returns.clone());
    }

    // 3. The profile's rules for this method, and its invariants.
    let method_id = profile
        .documents()
        .method(&vector.method)
        .map_or_else(|| vector.method.clone(), |method| method.id.clone());
    for rule in profile.documents().behaviors_for(&method_id) {
        for predicate in rule.preconditions.iter().chain(rule.postconditions.iter()) {
            collect_predicate(predicate, &mut expressions);
        }
    }
    for invariant in &profile.documents().invariants.invariants {
        if let Some(predicate) = &invariant.predicate {
            collect_predicate(predicate, &mut expressions);
        }
    }

    // 4. Every read those expressions name, resolved and performed.
    let references: Vec<&ValueExpr> = expressions.iter().collect();
    for (method, expression) in values::reads(&references) {
        let args = match values::read_arguments(&expression, &vector.inputs) {
            Ok(args) => args,
            Err(problem) => {
                // An argument that cannot be resolved before the call is one the
                // requirement cannot be evaluated against, so it is recorded as
                // unavailable rather than dropped. The read is keyed by what could be
                // worked out, which for an unresolved argument is nothing.
                record.insert_unavailable(
                    &method,
                    Vec::new(),
                    format!("its arguments could not be resolved before the operation: {problem}"),
                );
                continue;
            },
        };
        record_read(world, &mut record, &method, args);
    }

    record
}

/// Performs one read and records it, either way.
fn record_read(
    world: &ContractWorld<'_>,
    record: &mut RecordedWorld,
    method: &str,
    args: Vec<Value>,
) {
    match world.read(method, &args) {
        Ok(value) => record.insert(method, args, value),
        Err(problem) => record.insert_unavailable(method, args, problem.message().to_owned()),
    }
}

/// Collects the read expressions inside a predicate.
fn collect_predicate(predicate: &estamora_core::expr::Predicate, into: &mut Vec<ValueExpr>) {
    use estamora_core::expr::Predicate;
    match predicate {
        Predicate::Equal { left, right }
        | Predicate::NotEqual { left, right }
        | Predicate::LessThan { left, right }
        | Predicate::LessOrEqual { left, right }
        | Predicate::GreaterThan { left, right }
        | Predicate::GreaterOrEqual { left, right } => {
            into.push(left.clone());
            into.push(right.clone());
        },
        Predicate::OneOf { value, allowed } => {
            into.push(value.clone());
            into.extend(allowed.iter().cloned());
        },
        Predicate::InRange { value, min, max } => {
            into.push(value.clone());
            into.push(min.clone());
            into.push(max.clone());
        },
        Predicate::Delta { target, by, .. } => {
            into.push(target.clone());
            into.extend(by.iter().cloned());
        },
        Predicate::Unchanged { target } => into.push(target.clone()),
        Predicate::AllOf { predicates } | Predicate::AnyOf { predicates } => {
            for inner in predicates {
                collect_predicate(inner, into);
            }
        },
        Predicate::Not { predicate } => collect_predicate(predicate, into),
    }
}

/// The vocabulary a fixture's actors are recorded in.
///
/// Built here rather than in the execution module so that the record and the world it
/// was taken from cannot disagree about which address belongs to which name.
#[must_use]
pub fn actors(names: &[String], addresses: &BTreeMap<String, soroban_sdk::Address>) -> Vec<Actor> {
    names
        .iter()
        .filter_map(|name| {
            addresses.get(name).map(|address| Actor {
                name: name.clone(),
                address: address.clone(),
            })
        })
        .collect()
}

/// The allowances a fixture granted, in the shape the world takes.
#[must_use]
pub fn allowances(vector: &VectorDocument) -> Vec<AllowanceGrant> {
    vector
        .fixtures
        .allowances
        .iter()
        .map(|allowance| AllowanceGrant {
            from: allowance.from.clone(),
            spender: allowance.spender.clone(),
            live_until_ledger: allowance.live_until_ledger,
        })
        .collect()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use estamora_core::value::Value;
    use estamora_core::world::World;

    use super::RecordedWorld;

    #[test]
    fn a_relative_requirement_compares_against_the_record_and_not_the_live_state() {
        let mut record = RecordedWorld::empty(1_000);
        record.insert(
            "balance",
            vec![Value::Address("alice".to_owned())],
            Value::Integer(1_000),
        );
        assert_eq!(
            record
                .read("balance", &[Value::Address("alice".to_owned())])
                .unwrap(),
            Value::Integer(1_000)
        );
    }

    #[test]
    fn a_read_that_was_not_recorded_is_refused_rather_than_answered() {
        // An invented answer would be indistinguishable from a measurement, and a
        // requirement evaluated against it would be a requirement the runner did not
        // actually check.
        let record = RecordedWorld::empty(1_000);
        let problem = record
            .read("balance", &[Value::Address("alice".to_owned())])
            .unwrap_err();
        assert_eq!(problem.class(), estamora_core::ErrorClass::ExecutionError);
        assert!(problem.message().contains("before the operation"));
    }

    #[test]
    fn a_read_that_failed_is_reported_as_a_failure_of_the_reading() {
        let mut record = RecordedWorld::empty(1_000);
        record.insert_unavailable(
            "balance",
            vec![Value::Address("alice".to_owned())],
            "the contract has no such method",
        );
        let problem = record
            .read("balance", &[Value::Address("alice".to_owned())])
            .unwrap_err();
        assert!(problem.message().contains("no such method"));
    }

    #[test]
    fn a_resource_set_ranges_over_the_members_the_fixture_declared_in_its_order() {
        let mut record = RecordedWorld::empty(1_000);
        for (name, amount) in [("alice", 1_000), ("bob", 500)] {
            record.insert(
                "balance",
                vec![Value::Address(name.to_owned())],
                Value::Integer(amount),
            );
        }
        let members = record
            .resource_members(estamora_soroban::world::BALANCES)
            .unwrap();
        assert_eq!(
            members,
            vec![Value::Integer(1_000), Value::Integer(500)],
            "a run that names a member must name the same one every time"
        );
    }

    #[test]
    fn a_resource_set_that_was_never_recorded_is_refused_by_name() {
        let record = RecordedWorld::empty(1_000);
        let problem = record.resource_members("storage").unwrap_err();
        assert!(
            problem.message().contains("storage"),
            "{}",
            problem.message()
        );
    }

    #[test]
    fn an_allowance_expiry_is_answered_from_the_fixture_that_declared_it() {
        let mut record = RecordedWorld::empty(1_000);
        record.insert_expiry("alice", "carol", Some(5_000));
        assert_eq!(record.allowance_expiry("alice", "carol"), Some(5_000));
        assert_eq!(record.allowance_expiry("carol", "alice"), None);
    }

    #[test]
    fn recording_the_same_read_twice_keeps_the_first_answer() {
        // Two runs of a vector must record the same reads, and a later read must not
        // overwrite an earlier one — the state before the call is one value.
        let mut record = RecordedWorld::empty(1_000);
        record.insert("balance", Vec::new(), Value::Integer(1));
        record.insert("balance", Vec::new(), Value::Integer(2));
        assert_eq!(record.len(), 1);
        assert_eq!(record.read("balance", &[]).unwrap(), Value::Integer(1));
    }
}
