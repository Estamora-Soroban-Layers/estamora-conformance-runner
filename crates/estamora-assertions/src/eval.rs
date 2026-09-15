//! Interpreting a requirement.
//!
//! This is where the specification's expression algebra becomes a result. A
//! predicate is evaluated against an [`Environment`] — a world before the
//! operation, a world after it, the operation's inputs, and the ledger point — and
//! produces an [`Evaluation`]: whether it held, and the two strings a report needs
//! in order to be diagnosable without re-running anything.
//!
//! # Why every result carries expected and observed
//!
//! A conformance report is read by someone who was not present at the run, and
//! often by someone who cannot re-run it because the target is a network. A bare
//! "failed" would force them to reproduce the failure to learn anything, so every
//! evaluation renders both sides. That is also why the rendering is bounded: an
//! observation can come from a contract, and a report is a document other tools
//! parse.
//!
//! # Relative comparisons need two worlds
//!
//! `delta` and `unchanged` are statements about a change, and a change cannot be
//! read from one snapshot. The environment therefore carries both, and the *before*
//! world is what the target is evaluated against when a magnitude is needed. This
//! is the reason the runner takes a pre-operation snapshot at all rather than only
//! reading state afterwards.
//!
//! # Unbounded recursion is impossible by construction
//!
//! An input may be expressed in terms of another input, so resolving one involves
//! resolving the other. The algebra has no iteration, but it could still be written
//! as a cycle by an untrusted document, so resolution carries a depth bound and
//! refuses past it. The bound is a property of the document rather than of the
//! run's memory.

use std::collections::BTreeMap;
use std::fmt;

use estamora_core::expr::{ArithmeticOp, DeltaDirection, Literal, Predicate, ValueExpr};
use estamora_core::{Error, ErrorClass, Result};

use crate::value::{Comparison, Value};
use crate::world::World;

/// How deep an input may be defined in terms of other inputs.
///
/// A requirement is a document written by a person. Twenty levels is far beyond
/// anything readable, so exceeding it means a cycle rather than a style.
pub const MAX_RESOLUTION_DEPTH: usize = 20;

/// What a requirement is evaluated against.
pub struct Environment<'a> {
    /// The world before the operation ran, for relative comparisons.
    pub before: &'a dyn World,
    /// The world after it ran.
    pub after: &'a dyn World,
    /// The operation's inputs, as the vector declares them.
    pub inputs: &'a BTreeMap<String, ValueExpr>,
    /// The ledger sequence the operation executed at.
    pub ledger_sequence: u32,
}

/// Prints the ledger point and the input names, never the world itself.
///
/// A `World` is a handle onto an execution environment, and deriving a debug
/// rendering from one would put a value that differs between two runs of the same
/// vector into every diagnostic — the opposite of what this runner produces
/// anywhere else.
impl fmt::Debug for Environment<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Environment")
            .field("ledger_sequence", &self.ledger_sequence)
            .field("inputs", &self.inputs.keys().collect::<Vec<&String>>())
            .finish_non_exhaustive()
    }
}

/// The result of evaluating one requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluation {
    /// Whether the requirement held.
    pub held: bool,
    /// What the requirement asked for, rendered for a reader.
    pub expected: String,
    /// What was actually observed, rendered for a reader.
    pub observed: String,
}

impl Evaluation {
    /// An evaluation from a boolean, with both sides rendered.
    fn from_bool(held: bool, expected: impl Into<String>, observed: impl Into<String>) -> Self {
        Self {
            held,
            expected: expected.into(),
            observed: observed.into(),
        }
    }
}

/// Evaluates `predicate` against `environment`.
///
/// # Errors
///
/// Returns a vector error if the predicate refers to an input the vector does not
/// declare, or nests inputs past [`MAX_RESOLUTION_DEPTH`]. Returns an execution
/// error if a value could not be computed — a failed read, arithmetic that left the
/// representable range, or a field read from a value that has none. None of those
/// is a statement about the contract's behaviour.
pub fn evaluate_predicate(
    environment: &Environment<'_>,
    predicate: &Predicate,
) -> Result<Evaluation> {
    evaluate(environment, predicate, None)
}

/// Evaluates `predicate` for one member of a resource set.
///
/// Used by an invariant that ranges over a set: inside it, `resource_member` is
/// that member rather than the whole set. Passing the member explicitly rather than
/// storing it in the environment is what keeps an invariant's evaluation
/// independent of the order the members are visited in.
///
/// # Errors
///
/// As [`evaluate_predicate`].
pub fn evaluate_predicate_for_member(
    environment: &Environment<'_>,
    predicate: &Predicate,
    member: &Value,
) -> Result<Evaluation> {
    evaluate(environment, predicate, Some(member))
}

/// Computes the value of `expr`.
///
/// # Errors
///
/// As [`evaluate_predicate`].
pub fn evaluate_value(environment: &Environment<'_>, expr: &ValueExpr) -> Result<Value> {
    compute(environment, expr, None, 0)
}

/// Evaluates one predicate, optionally inside a resource-set member.
fn evaluate(
    environment: &Environment<'_>,
    predicate: &Predicate,
    member: Option<&Value>,
) -> Result<Evaluation> {
    match predicate {
        Predicate::Equal { left, right } => {
            compare(environment, left, right, member, |o| o == Comparison::Equal)
        },
        Predicate::NotEqual { left, right } => {
            compare(environment, left, right, member, |o| o != Comparison::Equal)
        },
        Predicate::LessThan { left, right } => {
            compare(environment, left, right, member, |o| o == Comparison::Less)
        },
        Predicate::LessOrEqual { left, right } => compare(environment, left, right, member, |o| {
            matches!(o, Comparison::Less | Comparison::Equal)
        }),
        Predicate::GreaterThan { left, right } => compare(environment, left, right, member, |o| {
            o == Comparison::Greater
        }),
        Predicate::GreaterOrEqual { left, right } => {
            compare(environment, left, right, member, |o| {
                matches!(o, Comparison::Greater | Comparison::Equal)
            })
        },
        Predicate::OneOf { value, allowed } => {
            let observed = compute(environment, value, member, 0)?;
            let mut permitted = Vec::with_capacity(allowed.len());
            let mut held = false;
            for candidate in allowed {
                let candidate = compute(environment, candidate, member, 0)?;
                held |= observed.equals(&candidate);
                permitted.push(candidate.render());
            }
            Ok(Evaluation::from_bool(
                held,
                format!("one of {}", permitted.join(", ")),
                observed.render(),
            ))
        },
        Predicate::InRange { value, min, max } => {
            let observed = compute(environment, value, member, 0)?;
            let lower = compute(environment, min, member, 0)?;
            let upper = compute(environment, max, member, 0)?;
            let held = observed.compare(&lower) != Comparison::Less
                && observed.compare(&upper) != Comparison::Greater
                && observed.compare(&lower) != Comparison::Incomparable;
            Ok(Evaluation::from_bool(
                held,
                format!("between {} and {}", lower.render(), upper.render()),
                observed.render(),
            ))
        },
        Predicate::Delta {
            target,
            direction,
            by,
        } => evaluate_delta(environment, target, *direction, by.as_ref(), member),
        Predicate::Unchanged { target } => {
            let before = compute(&environment.against_before(), target, member, 0)?;
            let after = compute(environment, target, member, 0)?;
            Ok(Evaluation::from_bool(
                before.equals(&after),
                format!("unchanged at {}", before.render()),
                format!("{} → {}", before.render(), after.render()),
            ))
        },
        Predicate::AllOf { predicates } => {
            let mut expected = Vec::with_capacity(predicates.len());
            let mut observed = Vec::with_capacity(predicates.len());
            let mut held = true;
            for inner in predicates {
                let result = evaluate(environment, inner, member)?;
                held &= result.held;
                expected.push(result.expected);
                observed.push(result.observed);
            }
            Ok(Evaluation::from_bool(
                held,
                format!("all of [{}]", expected.join("; ")),
                observed.join("; "),
            ))
        },
        Predicate::AnyOf { predicates } => {
            let mut expected = Vec::with_capacity(predicates.len());
            let mut observed = Vec::with_capacity(predicates.len());
            let mut held = false;
            for inner in predicates {
                let result = evaluate(environment, inner, member)?;
                held |= result.held;
                expected.push(result.expected);
                observed.push(result.observed);
            }
            Ok(Evaluation::from_bool(
                held,
                format!("any of [{}]", expected.join("; ")),
                observed.join("; "),
            ))
        },
        Predicate::Not { predicate } => {
            let result = evaluate(environment, predicate, member)?;
            Ok(Evaluation::from_bool(
                !result.held,
                format!("not ({})", result.expected),
                result.observed,
            ))
        },
    }
}

/// Renders a movement between two worlds.
fn evaluate_delta(
    environment: &Environment<'_>,
    target: &ValueExpr,
    direction: DeltaDirection,
    by: Option<&ValueExpr>,
    member: Option<&Value>,
) -> Result<Evaluation> {
    let before = compute(&environment.against_before(), target, member, 0)?;
    let after = compute(environment, target, member, 0)?;
    let movement = format!("{} → {}", before.render(), after.render());

    let (Some(from), Some(to)) = (before.as_integer(), after.as_integer()) else {
        // A relative requirement is a statement about a quantity, so it can only be
        // applied to something that is one.
        return Err(execution_error(format!(
            "a {} change was asserted of a value that is not a quantity: {movement}",
            match direction {
                DeltaDirection::Increase => "positive",
                DeltaDirection::Decrease => "negative",
                DeltaDirection::Unchanged => "zero",
            }
        )));
    };

    let observed_direction = match to.cmp(&from) {
        std::cmp::Ordering::Greater => DeltaDirection::Increase,
        std::cmp::Ordering::Less => DeltaDirection::Decrease,
        std::cmp::Ordering::Equal => DeltaDirection::Unchanged,
    };

    let mut held = observed_direction == direction;
    let mut expected = match direction {
        DeltaDirection::Increase => "increase".to_owned(),
        DeltaDirection::Decrease => "decrease".to_owned(),
        DeltaDirection::Unchanged => "unchanged".to_owned(),
    };

    if let Some(magnitude) = by {
        let required = compute(environment, magnitude, member, 0)?;
        let Some(required) = required.as_integer() else {
            return Err(execution_error(format!(
                "a change was required to equal {}, which is not a quantity",
                required.render()
            )));
        };
        // A stated magnitude *is* the requirement, direction included, so the
        // direction is not tested separately once one is written. The two tests are
        // not independent: an operation that moves nothing satisfies "decrease by
        // zero", because the balance did change — by minus zero — and a separate
        // direction test would read that movement as `unchanged` and reject it. The
        // consequence is not academic: it makes a conforming contract that transfers
        // an amount of zero fail a requirement it satisfies exactly.
        //
        // The subtraction is checked rather than plain, because the difference of
        // two representable integers can itself be unrepresentable. A difference
        // that cannot be written down cannot equal a magnitude that was, so the
        // requirement does not hold — which is a fact about the observation rather
        // than a reason to abort the evaluation. `checked_neg` for the same reason: a
        // magnitude at the bottom of the range has no representable negative.
        let signed = match direction {
            DeltaDirection::Increase => Some(required),
            DeltaDirection::Decrease => required.checked_neg(),
            // "Unchanged by a nonzero magnitude" is not a requirement about a
            // movement at all and cannot hold. Stated rather than folded into
            // "unchanged", which would read a contradictory requirement as a
            // satisfied one.
            DeltaDirection::Unchanged => (required == 0).then_some(0),
        };
        held = signed.is_some_and(|signed| to.checked_sub(from) == Some(signed));
        expected = format!("{expected} by {required}");
    }

    if !held {
        // The magnitude is the part a reader most often needs, so it is stated
        // rather than left to be recomputed from the two endpoints.
        expected = format!("{expected} (observed {})", to.saturating_sub(from));
    }

    Ok(Evaluation::from_bool(held, expected, movement))
}

/// Evaluates the two sides of a comparison and applies `holds`.
fn compare(
    environment: &Environment<'_>,
    left: &ValueExpr,
    right: &ValueExpr,
    member: Option<&Value>,
    holds: impl Fn(Comparison) -> bool,
) -> Result<Evaluation> {
    let observed = compute(environment, left, member, 0)?;
    let expected = compute(environment, right, member, 0)?;
    let comparison = observed.compare(&expected);
    // Expected and observed are rendered in the order the requirement was written,
    // because a reader is comparing the vector's text with the report.
    Ok(Evaluation::from_bool(
        holds(comparison),
        format!("{} ({})", expected.render(), describe(comparison)),
        observed.render(),
    ))
}

/// How a comparison is described in a report.
fn describe(comparison: Comparison) -> &'static str {
    match comparison {
        Comparison::Less => "observed is less",
        Comparison::Equal => "equal",
        Comparison::Greater => "observed is greater",
        Comparison::Incomparable => "not comparable to the observed value",
    }
}

/// Computes a value expression.
fn compute(
    environment: &Environment<'_>,
    expr: &ValueExpr,
    member: Option<&Value>,
    depth: usize,
) -> Result<Value> {
    if depth > MAX_RESOLUTION_DEPTH {
        return Err(Error::new(
            ErrorClass::VectorError,
            format!(
                "resolving an expression nested it more than {MAX_RESOLUTION_DEPTH} levels deep, \
                 which a readable requirement does not reach"
            ),
        ));
    }

    match expr {
        ValueExpr::Literal { value } => Ok(literal(value)),
        ValueExpr::Actor { actor } => Ok(Value::Address(actor.clone())),
        ValueExpr::Input { name } => {
            let declared = environment.inputs.get(name).ok_or_else(|| {
                Error::new(
                    ErrorClass::VectorError,
                    format!("the requirement refers to the input {name:?}, which is not declared"),
                )
            })?;
            compute(environment, declared, member, depth + 1)
        },
        ValueExpr::Read { method, args } => {
            let mut resolved = Vec::with_capacity(args.len());
            for argument in args {
                resolved.push(compute(environment, argument, member, depth + 1)?);
            }
            environment.after.read(method, &resolved)
        },
        ValueExpr::Sum { over } => {
            let members = environment.after.resource_members(over)?;
            let mut total: i128 = 0;
            for value in members {
                let Some(quantity) = value.as_integer() else {
                    return Err(execution_error(format!(
                        "the resource set {over:?} holds {}, which is not a quantity and so cannot \
                         be summed",
                        value.render()
                    )));
                };
                total = total.checked_add(quantity).ok_or_else(|| {
                    execution_error(format!(
                        "summing the resource set {over:?} left the representable range"
                    ))
                })?;
            }
            Ok(Value::Integer(total))
        },
        ValueExpr::Field { of, field } => {
            let container = compute(environment, of, member, depth + 1)?;
            match container {
                Value::Record(fields) => fields.get(field).cloned().ok_or_else(|| {
                    execution_error(format!(
                        "the value has no field {field:?}; it holds {}",
                        fields.keys().cloned().collect::<Vec<_>>().join(", ")
                    ))
                }),
                other => Err(execution_error(format!(
                    "the field {field:?} was read from {}, which has no fields",
                    other.render()
                ))),
            }
        },
        ValueExpr::LedgerSequence => Ok(Value::Integer(i128::from(environment.ledger_sequence))),
        ValueExpr::AllowanceExpiry { from, spender } => {
            let from = compute(environment, from, member, depth + 1)?;
            let spender = compute(environment, spender, member, depth + 1)?;
            let (Value::Address(from), Value::Address(spender)) = (&from, &spender) else {
                return Err(execution_error(format!(
                    "an allowance is named by two addresses, but {} and {} are not addresses",
                    from.render(),
                    spender.render()
                )));
            };
            Ok(match environment.after.allowance_expiry(from, spender) {
                Some(ledger) => Value::Integer(i128::from(ledger)),
                None => Value::Absent,
            })
        },
        ValueExpr::ResourceMember => member.cloned().ok_or_else(|| {
            Error::new(
                ErrorClass::VectorError,
                "`resource_member` is only meaningful inside an invariant that ranges over a \
                 resource set"
                    .to_owned(),
            )
        }),
        ValueExpr::Arithmetic { op, operands } => {
            let mut resolved = Vec::with_capacity(operands.len());
            for operand in operands {
                resolved.push(compute(environment, operand, member, depth + 1)?);
            }
            apply_arithmetic(*op, &resolved)
        },
    }
}

/// Converts a document literal into a value.
///
/// An integral number becomes an integer so that a vector may write `0` without
/// quotes; anything fractional stays text, because a requirement about a token
/// quantity is not a requirement about a float.
fn literal(value: &Literal) -> Value {
    match value {
        Literal::Text(text) => Value::Text(text.clone()),
        Literal::Bool(flag) => Value::Bool(*flag),
        Literal::Null => Value::Absent,
        // Rendered and re-read rather than cast, for two reasons: a cast from a
        // float to an integer is lossy in a way the compiler cannot see past, and
        // routing through the canonical-decimal rule means `2.0` and `2` reach the
        // same value while `2.5` stays text rather than being truncated to it.
        Literal::Number(number) => {
            let text = number.to_string();
            match Value::integer_from_text(&text) {
                Some(value) => Value::Integer(value),
                None => Value::Text(text),
            }
        },
    }
}

/// Applies an arithmetic operator to already-resolved operands.
fn apply_arithmetic(op: ArithmeticOp, operands: &[Value]) -> Result<Value> {
    let mut quantities = Vec::with_capacity(operands.len());
    for operand in operands {
        let Some(quantity) = operand.as_integer() else {
            return Err(execution_error(format!(
                "arithmetic was applied to {}, which is not a quantity",
                operand.render()
            )));
        };
        quantities.push(quantity);
    }

    let mut iter = quantities.into_iter();
    let Some(first) = iter.next() else {
        return Err(execution_error("arithmetic was applied to no operands"));
    };

    let mut total = first;
    for quantity in iter {
        total = match op {
            ArithmeticOp::Add => total.checked_add(quantity),
            ArithmeticOp::Subtract => total.checked_sub(quantity),
            ArithmeticOp::Multiply => total.checked_mul(quantity),
            ArithmeticOp::Divide => {
                if quantity == 0 {
                    return Err(execution_error("division by zero was required"));
                }
                total.checked_div(quantity)
            },
        }
        .ok_or_else(|| execution_error("arithmetic left the representable range"))?;
    }
    Ok(Value::Integer(total))
}

/// An execution failure: the observation could not be produced.
fn execution_error(message: impl Into<String>) -> Error {
    Error::new(ErrorClass::ExecutionError, message)
}

/// The same environment with both worlds pointing at the before world.
///
/// Relative comparisons evaluate their target against the *before* world, so the
/// target's own reads must reach it while the inputs and the ledger point stay the
/// same. Building the swapped environment in one place is what keeps a single
/// definition of which world a read reaches: a helper that merely returned `self`
/// would read the state the operation had already changed.
impl Environment<'_> {
    fn against_before(&self) -> Self {
        Self {
            before: self.before,
            after: self.before,
            inputs: self.inputs,
            ledger_sequence: self.ledger_sequence,
        }
    }
}
