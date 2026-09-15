//! Checking a vector against the profile it is declared for.
//!
//! A vector is not self-contained. It names a method the profile declares, inputs
//! that must exactly cover that method's arguments, failures and invariants and
//! events from the profile's own documents, and fixture actors its expressions
//! refer to. None of those is checkable from the vector alone, and every one of
//! them is a way for a requirement to be written, reviewed and then never applied.
//!
//! # Not resolving is a defect, not a robustness question
//!
//! A vector that names a failure the profile does not declare cannot be executed.
//! If it were skipped, the run would report a contract as conformant against a
//! corpus that quietly did not include it — the failure mode this project exists to
//! prevent. So an unresolvable reference is an error, and only one case is
//! different: a **profile-independent** vector whose method the profile does not
//! implement does not apply to that profile at all, and is excluded rather than
//! refused. See [`crate::loader::VectorCorpus::load`].

use std::collections::BTreeSet;

use estamora_core::expr::{Predicate, ValueExpr};
use estamora_core::{Diagnostic, Diagnostics, ErrorClass};
use estamora_profile::ProfileBundle;

use crate::model::{AuthorizationExpectation, ExpectedResult, StateResourceRef, VectorDocument};

/// Reports every reference in `vector` that the profile cannot resolve.
#[must_use]
pub fn validate(bundle: &ProfileBundle, vector: &VectorDocument) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    let documents = bundle.documents();
    let wildcard = vector.profile == "*";

    // The identity of the requirement set the vector is attributed to. A vector
    // found under one profile while declaring another means a result would be
    // reported against requirements it was never written for.
    if !wildcard && vector.profile != bundle.document().profile.id {
        fail(
            &mut diagnostics,
            format!(
                "vector {:?} declares profile {:?} but was found under {:?}",
                vector.id,
                vector.profile,
                bundle.document().profile.id
            ),
            "vector",
            vector.id.as_str(),
        );
    }

    // A wildcard vector belongs to no profile, so the only version it can be
    // pinned to is the format's. A profile-owned vector is pinned to the profile's
    // own revision, which is independent of the format's and of the runner's.
    let expected_version = if wildcard {
        bundle.spec_version_text()
    } else {
        bundle.document().profile.version.as_str()
    };
    if vector.profile_version != expected_version {
        fail(
            &mut diagnostics,
            format!(
                "vector {:?} was written against version {:?} but the requirement set is at \
                 {expected_version:?}",
                vector.id, vector.profile_version
            ),
            "vector",
            vector.id.as_str(),
        );
    }

    let actors = check_actors(&mut diagnostics, vector);

    // The method is the anchor for everything else, so an unresolvable one ends
    // the check: the remaining findings would all be consequences of it.
    let Some(method) = documents.method(&vector.method) else {
        fail(
            &mut diagnostics,
            format!(
                "vector {:?} exercises method {:?}, which the profile does not declare",
                vector.id, vector.method
            ),
            "method",
            vector.method.as_str(),
        );
        return diagnostics;
    };

    check_inputs(&mut diagnostics, vector, method);
    check_expectations(&mut diagnostics, bundle, vector);
    check_expressions(&mut diagnostics, bundle, vector, &actors);

    diagnostics
}

/// Collects the fixture actor names, reporting any declared twice.
fn check_actors(diagnostics: &mut Diagnostics, vector: &VectorDocument) -> BTreeSet<String> {
    let mut actors = BTreeSet::new();
    for actor in &vector.fixtures.actors {
        if !actors.insert(actor.name.clone()) {
            fail(
                diagnostics,
                format!(
                    "vector {:?} declares the fixture actor {:?} more than once; every reference \
                     to it is ambiguous",
                    vector.id, actor.name
                ),
                "actor",
                actor.name.as_str(),
            );
        }
    }

    for account in vector.fixtures.balances.keys() {
        require_actor(
            diagnostics,
            &actors,
            account,
            &vector.id,
            "an opening balance",
        );
    }
    for allowance in &vector.fixtures.allowances {
        require_actor(
            diagnostics,
            &actors,
            &allowance.from,
            &vector.id,
            "a fixture allowance's holder",
        );
        require_actor(
            diagnostics,
            &actors,
            &allowance.spender,
            &vector.id,
            "a fixture allowance's spender",
        );
    }
    for actor in vector.fixtures.authorization.keys() {
        require_actor(
            diagnostics,
            &actors,
            actor,
            &vector.id,
            "an authorization flag",
        );
    }
    for actor in vector
        .authorization
        .actors
        .iter()
        .chain(vector.authorization.substituted_for.iter())
    {
        require_actor(
            diagnostics,
            &actors,
            actor,
            &vector.id,
            "the authorization plan",
        );
    }

    actors
}

/// Reports a reference to a fixture actor the vector does not declare.
fn require_actor(
    diagnostics: &mut Diagnostics,
    actors: &BTreeSet<String>,
    name: &str,
    vector: &str,
    what: &str,
) {
    if !actors.contains(name) {
        fail(
            diagnostics,
            format!(
                "vector {vector:?} names {name:?} in {what}, but declares no such fixture actor"
            ),
            "actor",
            name,
        );
    }
}

/// Checks that the inputs cover the method's arguments exactly.
///
/// Exact coverage in *both* directions, which is stricter than a minimum count and
/// is what makes the vector deterministic: an input the method does not take would
/// be silently dropped, and an argument the vector does not supply would be
/// whatever the runner invented.
fn check_inputs(
    diagnostics: &mut Diagnostics,
    vector: &VectorDocument,
    method: &estamora_profile::MethodDefinition,
) {
    let declared: BTreeSet<&str> = method.args.iter().map(|arg| arg.name.as_str()).collect();
    let supplied: BTreeSet<&str> = vector.inputs.keys().map(String::as_str).collect();

    for name in supplied.difference(&declared) {
        fail(
            diagnostics,
            format!(
                "vector {:?} supplies the input {name:?}, which {:?} does not take",
                vector.id, method.name
            ),
            "input",
            name,
        );
    }
    for name in declared.difference(&supplied) {
        fail(
            diagnostics,
            format!(
                "vector {:?} does not supply {name:?}, which {:?} requires; the call it describes \
                 cannot be made",
                vector.id, method.name
            ),
            "argument",
            name,
        );
    }
}

/// Checks the expected outcome's references into the profile.
fn check_expectations(
    diagnostics: &mut Diagnostics,
    bundle: &ProfileBundle,
    vector: &VectorDocument,
) {
    let documents = bundle.documents();
    let expected = &vector.expected;

    match expected.outcome {
        ExpectedResult::Success => {
            // A vector that asserts both that a call succeeded and that it failed
            // is not a stricter requirement; it is an unsatisfiable one.
            for (field, present) in [
                ("failure", expected.failure.is_some()),
                ("failure_category", expected.failure_category.is_some()),
            ] {
                if present {
                    fail(
                        diagnostics,
                        format!(
                            "vector {:?} expects success but names a {field}, so it asserts both \
                             outcomes at once",
                            vector.id
                        ),
                        "field",
                        field,
                    );
                }
            }
        },
        ExpectedResult::Failure => {
            if expected.failure.is_none() && expected.failure_category.is_none() {
                fail(
                    diagnostics,
                    format!(
                        "vector {:?} expects failure but names no failure and no failure category, \
                         so any failure — including an unrelated one — would satisfy it",
                        vector.id
                    ),
                    "field",
                    "failure",
                );
            }
            if !expected.events.required.is_empty() {
                fail(
                    diagnostics,
                    format!(
                        "vector {:?} expects failure and also requires an event, but a refused \
                         operation must not have produced one",
                        vector.id
                    ),
                    "field",
                    "events.required",
                );
            }
        },
    }

    if let Some(failure) = &expected.failure
        && documents.failure(failure).is_none()
    {
        fail(
            diagnostics,
            format!(
                "vector {:?} expects the failure {failure:?}, which the profile does not declare",
                vector.id
            ),
            "failure",
            failure.as_str(),
        );
    }

    for invariant in &expected.invariants {
        if documents.invariant(invariant).is_none() {
            fail(
                diagnostics,
                format!(
                    "vector {:?} requires the invariant {invariant:?}, which the profile does not \
                     declare",
                    vector.id
                ),
                "invariant",
                invariant.as_str(),
            );
        }
    }

    for event in expected
        .events
        .required
        .iter()
        .map(|required| &required.event)
        .chain(expected.events.forbidden.iter())
    {
        if documents.event(event).is_none() {
            fail(
                diagnostics,
                format!(
                    "vector {:?} refers to the event {event:?}, which the profile does not declare",
                    vector.id
                ),
                "event",
                event.as_str(),
            );
        }
    }

    // The plan's own consistency: a call nobody signs cannot be described as
    // accepted-by-signature, and a call that must not require authorization cannot
    // have signers listed.
    match vector.authorization.expected {
        AuthorizationExpectation::NotRequired if !vector.authorization.actors.is_empty() => {
            fail(
                diagnostics,
                format!(
                    "vector {:?} expects authorization to be unnecessary but lists {} signer(s)",
                    vector.id,
                    vector.authorization.actors.len()
                ),
                "field",
                "authorization.actors",
            );
        },
        AuthorizationExpectation::Accepted
        | AuthorizationExpectation::Rejected
        | AuthorizationExpectation::NotRequired => {},
    }
}

/// Checks every fixture and method name an expression refers to.
fn check_expressions(
    diagnostics: &mut Diagnostics,
    bundle: &ProfileBundle,
    vector: &VectorDocument,
    actors: &BTreeSet<String>,
) {
    let documents = bundle.documents();
    let mut referenced_actors = BTreeSet::new();
    let mut reads = BTreeSet::new();

    for input in vector.inputs.values() {
        walk_value(input, &mut referenced_actors, &mut reads);
    }
    if let Some(returns) = &vector.expected.returns {
        walk_value(returns, &mut referenced_actors, &mut reads);
    }
    for expectation in &vector.expected.events.required {
        for topic in &expectation.topics {
            walk_value(topic, &mut referenced_actors, &mut reads);
        }
        for datum in &expectation.data {
            walk_value(datum, &mut referenced_actors, &mut reads);
        }
    }
    for assertion in &vector.expected.state_assertions {
        if let StateResourceRef::Custom { read, .. } = &assertion.resource {
            walk_value(read, &mut referenced_actors, &mut reads);
        }
        walk_predicate(&assertion.predicate, &mut referenced_actors, &mut reads);
    }
    for assertion in &vector.assertions {
        walk_predicate(&assertion.predicate, &mut referenced_actors, &mut reads);
    }

    for actor in referenced_actors {
        if !actors.contains(actor.as_str()) {
            fail(
                diagnostics,
                format!(
                    "vector {:?} refers to the fixture actor {actor:?}, which it does not declare; \
                     a run would be measuring a world the vector never described",
                    vector.id
                ),
                "actor",
                actor.as_str(),
            );
        }
    }

    for method in reads {
        if documents.method(&method).is_none() {
            fail(
                diagnostics,
                format!(
                    "vector {:?} reads {method:?} to observe state, but the profile does not declare \
                     that method; the observation cannot be made",
                    vector.id
                ),
                "read",
                method.as_str(),
            );
        }
    }
}

/// Collects the fixture actors and read methods a value expression refers to.
fn walk_value(expr: &ValueExpr, actors: &mut BTreeSet<String>, reads: &mut BTreeSet<String>) {
    match expr {
        ValueExpr::Literal { .. }
        | ValueExpr::Input { .. }
        | ValueExpr::Sum { .. }
        | ValueExpr::LedgerSequence
        | ValueExpr::ResourceMember => {},
        ValueExpr::Actor { actor } => {
            actors.insert(actor.clone());
        },
        ValueExpr::Read { method, args } => {
            reads.insert(method.clone());
            for argument in args {
                walk_value(argument, actors, reads);
            }
        },
        ValueExpr::Field { of, .. } => walk_value(of, actors, reads),
        ValueExpr::AllowanceExpiry { from, spender } => {
            walk_value(from, actors, reads);
            walk_value(spender, actors, reads);
        },
        ValueExpr::Arithmetic { operands, .. } => {
            for operand in operands {
                walk_value(operand, actors, reads);
            }
        },
    }
}

/// Collects the fixture actors and read methods a predicate refers to.
fn walk_predicate(
    predicate: &Predicate,
    actors: &mut BTreeSet<String>,
    reads: &mut BTreeSet<String>,
) {
    match predicate {
        Predicate::Equal { left, right }
        | Predicate::NotEqual { left, right }
        | Predicate::LessThan { left, right }
        | Predicate::LessOrEqual { left, right }
        | Predicate::GreaterThan { left, right }
        | Predicate::GreaterOrEqual { left, right } => {
            walk_value(left, actors, reads);
            walk_value(right, actors, reads);
        },
        Predicate::OneOf { value, allowed } => {
            walk_value(value, actors, reads);
            for candidate in allowed {
                walk_value(candidate, actors, reads);
            }
        },
        Predicate::InRange { value, min, max } => {
            walk_value(value, actors, reads);
            walk_value(min, actors, reads);
            walk_value(max, actors, reads);
        },
        Predicate::Delta { target, by, .. } => {
            walk_value(target, actors, reads);
            if let Some(magnitude) = by {
                walk_value(magnitude, actors, reads);
            }
        },
        Predicate::Unchanged { target } => walk_value(target, actors, reads),
        Predicate::AllOf { predicates } | Predicate::AnyOf { predicates } => {
            for inner in predicates {
                walk_predicate(inner, actors, reads);
            }
        },
        Predicate::Not { predicate } => walk_predicate(predicate, actors, reads),
    }
}

/// Records an error-severity finding.
fn fail(diagnostics: &mut Diagnostics, message: String, field: &str, value: &str) {
    diagnostics.push(
        Diagnostic::error(ErrorClass::VectorError, message)
            .with_context("field", field.to_owned())
            .with_context("reference", value.to_owned()),
    );
}
