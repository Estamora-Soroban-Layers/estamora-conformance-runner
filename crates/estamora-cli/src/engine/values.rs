//! Turning a vector's declared inputs and every read a requirement names into the
//! values an execution is made of.
//!
//! Two jobs live here, and they are different in kind.
//!
//! # Inputs are typed by the profile, not guessed from the document
//!
//! A vector writes an amount as `"250"` and a profile declares the argument as
//! `i128`. The runner therefore parses each input **according to the type the profile
//! declares**, rather than by sniffing the text: a document decides what value it
//! means, and the declaration decides how to read it. Guessing would turn `"0250"`
//! into 250 for one implementation and into text for another, and two implementations
//! of one vector must reach the same conclusion.
//!
//! A value of the wrong shape for its declared type is a vector defect and is refused
//! rather than coerced. Nothing here may silently reinterpret a document.
//!
//! # Reads are collected, because the before-world has to be recorded before the call
//!
//! A requirement can say the balance *fell* by the amount, which is a statement about
//! a change and therefore needs both worlds. The world after the call is a live view
//! of the contract; the world before it has to be captured while it is still true, so
//! every read a requirement might perform has to be known before the call is made.
//! [`reads`] walks the profile's rules and the vector's expectations for those.

use std::collections::BTreeMap;

use estamora_core::expr::{Predicate, ValueExpr};
use estamora_core::value::Value;
use estamora_core::{Error, ErrorClass, Result};
use estamora_profile::types::{MethodDefinition, PrimitiveType, TypeExpr};

/// One read a requirement performs, as `(method, arguments)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    /// The method called.
    pub method: String,
    /// The arguments, in call order.
    pub args: Vec<Value>,
}

/// Resolves an input to the value the profile's declaration says it is.
///
/// # Errors
///
/// Returns a vector error when the expression is not one a vector may use for an
/// argument, when an input it names is not declared, or when its value cannot be read
/// as the declared type. Every one of those is a defect in the document rather than
/// anything about the contract.
pub fn argument(
    expression: &ValueExpr,
    declared: &TypeExpr,
    inputs: &BTreeMap<String, ValueExpr>,
) -> Result<Value> {
    let scalar = scalar(expression, inputs)?;
    typed(scalar, declared, expression)
}

/// Resolves a self-contained expression to a value.
///
/// Only the three forms that name something already known are accepted: a literal, a
/// fixture actor, and a declared input. A read, an aggregate or a ledger fact is not
/// resolvable at this point in the pipeline, because resolving one would require the
/// execution that is being prepared; the failure says so rather than returning a
/// placeholder.
///
/// # Errors
///
/// Returns a vector error for an unresolvable form or an undeclared input.
pub fn scalar(expression: &ValueExpr, inputs: &BTreeMap<String, ValueExpr>) -> Result<Value> {
    match expression {
        ValueExpr::Literal { value } => Ok(match value {
            estamora_core::expr::Literal::Text(text) => {
                // A quoted amount is still a quantity where a quantity is what the
                // declaration asks for; that decision is made in [`typed`], so the
                // literal is carried here as the document wrote it.
                Value::Text(text.clone())
            },
            estamora_core::expr::Literal::Number(number) => number
                .to_string()
                .parse::<i128>()
                .map_or_else(|_| Value::Text(number.to_string()), Value::Integer),
            estamora_core::expr::Literal::Bool(flag) => Value::Bool(*flag),
            estamora_core::expr::Literal::Null => Value::Absent,
        }),
        ValueExpr::Actor { actor } => Ok(Value::Address(actor.clone())),
        ValueExpr::Input { name } => {
            let inner = inputs.get(name).ok_or_else(|| {
                Error::new(
                    ErrorClass::VectorError,
                    format!(
                        "the argument expression names the input {name:?}, and the vector does \
                         not declare it"
                    ),
                )
            })?;
            if matches!(inner, ValueExpr::Input { name: other } if other == name) {
                return Err(Error::new(
                    ErrorClass::VectorError,
                    format!("the input {name:?} is defined as itself"),
                ));
            }
            scalar(inner, inputs)
        },
        other => Err(Error::new(
            ErrorClass::VectorError,
            format!(
                "an argument of the operation must be a literal, a fixture actor or a declared \
                 input, and this one is a {} expression; a value that depends on the contract \
                 cannot be used as an input to it",
                describe(other)
            ),
        )),
    }
}

/// Reads a scalar as the type the profile declares for it.
///
/// # Errors
///
/// Returns a vector error when the value cannot be that type.
fn typed(scalar: Value, declared: &TypeExpr, expression: &ValueExpr) -> Result<Value> {
    match declared {
        TypeExpr::Prim { name, .. } => match name {
            PrimitiveType::I128
            | PrimitiveType::U128
            | PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::I32
            | PrimitiveType::U32
            | PrimitiveType::Timepoint
            | PrimitiveType::Duration => match scalar {
                Value::Integer(number) => Ok(Value::Integer(number)),
                Value::Text(text) => Value::integer_from_text(&text)
                    .map(Value::Integer)
                    .ok_or_else(|| not_of_type("an integer", &text, expression, *name)),
                other => Err(not_of_type(
                    "an integer",
                    &other.render(),
                    expression,
                    *name,
                )),
            },
            PrimitiveType::Address | PrimitiveType::MuxedAddress => match scalar {
                // A bare text value carries as an address exactly as an already-typed
                // one does, rather than being refused here: the execution layer decides
                // whether it names a fixture actor, and that is the layer that knows the
                // vocabulary. A check at this point would be a second, weaker copy of
                // it.
                Value::Address(name) | Value::Text(name) => Ok(Value::Address(name)),
                other => Err(not_of_type(
                    "an address",
                    &other.render(),
                    expression,
                    *name,
                )),
            },
            PrimitiveType::Bool => match scalar {
                Value::Bool(flag) => Ok(Value::Bool(flag)),
                Value::Text(text) => match text.as_str() {
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    _ => Err(not_of_type("a boolean", &text, expression, *name)),
                },
                other => Err(not_of_type("a boolean", &other.render(), expression, *name)),
            },
            PrimitiveType::Void => Err(Error::new(
                ErrorClass::VectorError,
                format!(
                    "the argument {} is declared as taking no value, so it cannot be supplied",
                    expression_name(expression)
                ),
            )),
            _ => Ok(scalar),
        },
        TypeExpr::Vec { element } => match scalar {
            Value::Sequence(items) => items
                .into_iter()
                .map(|item| typed(item, element, expression))
                .collect::<Result<Vec<Value>>>()
                .map(Value::Sequence),
            other => Err(not_of_type(
                "a sequence",
                &other.render(),
                expression,
                PrimitiveType::Val,
            )),
        },
        TypeExpr::Option { .. } => Ok(scalar),
        other => Err(Error::new(
            ErrorClass::VectorError,
            format!(
                "the argument {} is declared as {} and this runner cannot construct one from a \
                 vector; a vector may supply a primitive, an address or a sequence of them",
                expression_name(expression),
                estamora_assertions::canonical_type(other)
            ),
        )),
    }
}

/// A value that is not the type the profile declared.
fn not_of_type(
    expected: &str,
    found: &str,
    expression: &ValueExpr,
    declared: PrimitiveType,
) -> Error {
    Error::new(
        ErrorClass::VectorError,
        format!(
            "the argument {} is declared as {declared:?} and so must be {expected}, and {} is not",
            expression_name(expression),
            if found.is_empty() { "the value" } else { found }
        ),
    )
}

/// A short name for an expression, for a diagnostic.
fn expression_name(expression: &ValueExpr) -> String {
    match expression {
        ValueExpr::Actor { actor } => format!("`{actor}`"),
        ValueExpr::Input { name } => format!("the input `{name}`"),
        ValueExpr::Literal { value } => {
            format!("the literal {}", value.as_text().unwrap_or("<value>"))
        },
        other => format!("a {} expression", describe(other)),
    }
}

/// The document's name for an expression kind.
fn describe(expression: &ValueExpr) -> &'static str {
    match expression {
        ValueExpr::Literal { .. } => "literal",
        ValueExpr::Actor { .. } => "actor",
        ValueExpr::Input { .. } => "input",
        ValueExpr::Read { .. } => "read",
        ValueExpr::Sum { .. } => "sum",
        ValueExpr::Field { .. } => "field",
        ValueExpr::LedgerSequence => "ledger_sequence",
        ValueExpr::AllowanceExpiry { .. } => "allowance_expiry",
        ValueExpr::ResourceMember => "resource_member",
        ValueExpr::Arithmetic { .. } => "arithmetic",
    }
}

/// Every read a set of expressions performs.
///
/// Returned sorted and deduplicated, so that two runs record the same reads in the
/// same order and a failure names the same one.
#[must_use]
pub fn reads(expressions: &[&ValueExpr]) -> Vec<(String, ValueExpr)> {
    let mut found: Vec<(String, ValueExpr)> = Vec::new();
    for expression in expressions {
        collect_value(expression, &mut found);
    }
    found.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| format!("{:?}", left.1).cmp(&format!("{:?}", right.1)))
    });
    found.dedup_by(|left, right| {
        left.0 == right.0 && format!("{:?}", left.1) == format!("{:?}", right.1)
    });
    found
}

/// Every read a predicate performs.
#[must_use]
pub fn reads_in_predicate(predicate: &Predicate) -> Vec<(String, ValueExpr)> {
    let mut expressions: Vec<&ValueExpr> = Vec::new();
    collect_predicate(predicate, &mut expressions);
    reads(&expressions)
}

/// Collects the read expressions inside a value expression, in place.
fn collect_value(expression: &ValueExpr, found: &mut Vec<(String, ValueExpr)>) {
    match expression {
        ValueExpr::Read { method, args } => {
            found.push((method.clone(), expression.clone()));
            for argument in args {
                collect_value(argument, found);
            }
        },
        ValueExpr::Field { of, .. } => collect_value(of, found),
        ValueExpr::AllowanceExpiry { from, spender } => {
            collect_value(from, found);
            collect_value(spender, found);
        },
        ValueExpr::Arithmetic { operands, .. } => {
            for operand in operands {
                collect_value(operand, found);
            }
        },
        ValueExpr::Literal { .. }
        | ValueExpr::Actor { .. }
        | ValueExpr::Input { .. }
        | ValueExpr::Sum { .. }
        | ValueExpr::LedgerSequence
        | ValueExpr::ResourceMember => {},
    }
}

/// Collects the read expressions inside a predicate, in place.
fn collect_predicate<'a>(predicate: &'a Predicate, found: &mut Vec<&'a ValueExpr>) {
    match predicate {
        Predicate::Equal { left, right }
        | Predicate::NotEqual { left, right }
        | Predicate::LessThan { left, right }
        | Predicate::LessOrEqual { left, right }
        | Predicate::GreaterThan { left, right }
        | Predicate::GreaterOrEqual { left, right } => {
            found.push(left);
            found.push(right);
        },
        Predicate::OneOf { value, allowed } => {
            found.push(value);
            found.extend(allowed.iter());
        },
        Predicate::InRange { value, min, max } => {
            found.push(value);
            found.push(min);
            found.push(max);
        },
        Predicate::Delta { target, by, .. } => {
            found.push(target);
            found.extend(by.iter());
        },
        Predicate::Unchanged { target } => found.push(target),
        Predicate::AllOf { predicates } | Predicate::AnyOf { predicates } => {
            for inner in predicates {
                collect_predicate(inner, found);
            }
        },
        Predicate::Not { predicate } => collect_predicate(predicate, found),
    }
}

/// Resolves a read expression's arguments.
///
/// # Errors
///
/// Returns a vector error when an argument is not one this runner can resolve before
/// a call, which is the same restriction [`scalar`] applies.
pub fn read_arguments(
    expression: &ValueExpr,
    inputs: &BTreeMap<String, ValueExpr>,
) -> Result<Vec<Value>> {
    let ValueExpr::Read { args, .. } = expression else {
        return Err(Error::new(
            ErrorClass::InternalError,
            "a read was expected and something else was passed to the read resolver",
        ));
    };
    args.iter().map(|arg| scalar(arg, inputs)).collect()
}

/// Converts a resolved argument into the host value the declared type calls for.
///
/// # Why the conversion is driven by the declaration and not by the value
///
/// Soroban values are typed on the wire. An `i128` and a `u32` that both hold `30` are
/// different host values, and passing the wrong one does not produce a wrong answer — it
/// produces a call the host refuses before the contract runs. A scenario whose argument
/// conversion failed would then look exactly like a contract that rejected the call, and
/// the authorization and behaviour dimensions would both report a defect that is the
/// runner's.
///
/// So the profile's declaration decides, once, and everything the runner cannot represent
/// is refused by name rather than approximated.
///
/// # Errors
///
/// Returns a vector error when the value does not fit the declared type, and an execution
/// failure for an address that is not one of the fixture's actors. The first is a defect
/// in a document; the second is the runner's inability to make the call, and neither may
/// be reported as a contract finding.
pub fn host_argument(
    env: &soroban_sdk::Env,
    world: &estamora_soroban::ContractWorld<'_>,
    value: &Value,
    declared: &TypeExpr,
) -> Result<soroban_sdk::Val> {
    use soroban_sdk::IntoVal as _;

    let TypeExpr::Prim { name, .. } = declared else {
        // A composed type is carried by the world, which owns the vocabulary mapping and
        // already knows how to turn a sequence or a record into a host value.
        return world.to_host(value);
    };

    let integer = |what: PrimitiveType| -> Result<i128> {
        value.as_integer().ok_or_else(|| {
            Error::new(
                ErrorClass::VectorError,
                format!(
                    "an argument declared as {what:?} must be a quantity, and {} is not",
                    value.render()
                ),
            )
        })
    };

    Ok(match name {
        PrimitiveType::I128 | PrimitiveType::Timepoint | PrimitiveType::Duration => {
            integer(*name)?.into_val(env)
        },
        PrimitiveType::U128 => u128::try_from(integer(*name)?)
            .map_err(|_| negative_for(*name))?
            .into_val(env),
        PrimitiveType::I64 => i64::try_from(integer(*name)?)
            .map_err(|_| out_of_range(*name))?
            .into_val(env),
        PrimitiveType::U64 => u64::try_from(integer(*name)?)
            .map_err(|_| negative_for(*name))?
            .into_val(env),
        PrimitiveType::I32 => i32::try_from(integer(*name)?)
            .map_err(|_| out_of_range(*name))?
            .into_val(env),
        PrimitiveType::U32 => u32::try_from(integer(*name)?)
            .map_err(|_| negative_for(*name))?
            .into_val(env),
        PrimitiveType::Bool => value
            .as_bool()
            .ok_or_else(|| {
                Error::new(
                    ErrorClass::VectorError,
                    format!(
                        "an argument declared as bool must be a boolean, and {} is not",
                        value.render()
                    ),
                )
            })?
            .into_val(env),
        PrimitiveType::Symbol => match value {
            Value::Text(text) => soroban_sdk::Symbol::new(env, text).into_val(env),
            other => {
                return Err(Error::new(
                    ErrorClass::VectorError,
                    format!(
                        "an argument declared as symbol must be text, and {} is not",
                        other.render()
                    ),
                ));
            },
        },
        PrimitiveType::String => match value {
            Value::Text(text) => soroban_sdk::String::from_str(env, text).into_val(env),
            other => {
                return Err(Error::new(
                    ErrorClass::VectorError,
                    format!(
                        "an argument declared as string must be text, and {} is not",
                        other.render()
                    ),
                ));
            },
        },
        // The world owns the mapping from a fixture actor's name to the address it was
        // deployed as, so the conversion goes through it rather than repeating the rule
        // here where the two could disagree.
        PrimitiveType::Address | PrimitiveType::MuxedAddress | PrimitiveType::Val => {
            world.to_host(value)?
        },
        PrimitiveType::Bytes | PrimitiveType::BytesN => {
            return Err(Error::new(
                ErrorClass::VectorError,
                format!(
                    "an argument declared as {name:?} cannot be supplied by a vector: the format \
                     has no byte-string literal, and inventing one would be a second, weaker \
                     encoding of the same value"
                ),
            ));
        },
        PrimitiveType::Void => {
            return Err(Error::new(
                ErrorClass::VectorError,
                "an argument declared as taking no value cannot be supplied",
            ));
        },
    })
}

/// A quantity that is negative where its declared type cannot be.
fn negative_for(name: PrimitiveType) -> Error {
    Error::new(
        ErrorClass::VectorError,
        format!("an argument declared as {name:?} cannot be negative"),
    )
}

/// A quantity that does not fit the declared width.
fn out_of_range(name: PrimitiveType) -> Error {
    Error::new(
        ErrorClass::VectorError,
        format!("an argument declared as {name:?} does not fit its width"),
    )
}

/// The declared argument of `method` called `name`, where there is one.
#[must_use]
pub fn declared_argument<'a>(method: &'a MethodDefinition, name: &str) -> Option<&'a TypeExpr> {
    method
        .args
        .iter()
        .find(|argument| argument.name == name)
        .map(|argument| &argument.type_expr)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use std::collections::BTreeMap;

    use estamora_core::ErrorClass;
    use estamora_core::expr::{Literal, Predicate, ValueExpr};
    use estamora_core::value::Value;
    use estamora_profile::types::{PrimitiveType, TypeExpr};

    use super::{argument, reads_in_predicate, scalar};

    fn text(value: &str) -> ValueExpr {
        ValueExpr::Literal {
            value: Literal::Text(value.to_owned()),
        }
    }

    fn primitive(name: PrimitiveType) -> TypeExpr {
        TypeExpr::Prim { name, width: None }
    }

    #[test]
    fn an_amount_is_read_as_the_type_the_profile_declares_and_not_as_text() {
        // The decisive property: a quoted amount is a quantity where a quantity is
        // what the profile declares, and text where text is.
        let inputs = BTreeMap::new();
        assert_eq!(
            argument(&text("250"), &primitive(PrimitiveType::I128), &inputs).unwrap(),
            Value::Integer(250)
        );
        assert_eq!(
            argument(&text("250"), &primitive(PrimitiveType::String), &inputs).unwrap(),
            Value::Text("250".to_owned())
        );
    }

    #[test]
    fn a_number_that_is_not_canonical_is_refused_rather_than_reinterpreted() {
        // `"0250"` and `"250"` are two spellings of one number, and accepting both
        // would make two documents that differ compare equal.
        let inputs = BTreeMap::new();
        let problem =
            argument(&text("0250"), &primitive(PrimitiveType::I128), &inputs).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::VectorError);
        assert!(
            problem.message().contains("integer"),
            "{}",
            problem.message()
        );
    }

    #[test]
    fn an_actor_is_an_address_and_never_an_account_string() {
        let inputs = BTreeMap::new();
        let expression = ValueExpr::Actor {
            actor: "alice".to_owned(),
        };
        assert_eq!(
            argument(&expression, &primitive(PrimitiveType::Address), &inputs).unwrap(),
            Value::Address("alice".to_owned())
        );
    }

    #[test]
    fn an_input_that_names_itself_is_refused_rather_than_recursing_forever() {
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "amount".to_owned(),
            ValueExpr::Input {
                name: "amount".to_owned(),
            },
        );
        let problem = scalar(
            &ValueExpr::Input {
                name: "amount".to_owned(),
            },
            &inputs,
        )
        .unwrap_err();
        assert_eq!(problem.class(), ErrorClass::VectorError);
    }

    #[test]
    fn an_input_that_reads_the_contract_is_refused_as_a_vector_defect() {
        // A value that depends on the contract cannot be used as an input to it: the
        // execution it would need is the one being prepared.
        let inputs = BTreeMap::new();
        let expression = ValueExpr::Read {
            method: "balance".to_owned(),
            args: vec![ValueExpr::Actor {
                actor: "alice".to_owned(),
            }],
        };
        let problem = scalar(&expression, &inputs).unwrap_err();
        assert_eq!(problem.class(), ErrorClass::VectorError);
        assert!(problem.message().contains("read"), "{}", problem.message());
    }

    #[test]
    fn every_read_a_requirement_names_is_found_including_nested_ones() {
        // This is what makes the before-world complete: a read the collector missed is
        // a read the before-world cannot answer, and a relative requirement would then
        // fail for the runner's reason rather than the contract's.
        let predicate = Predicate::AllOf {
            predicates: vec![
                Predicate::Unchanged {
                    target: ValueExpr::Read {
                        method: "balance".to_owned(),
                        args: vec![ValueExpr::Actor {
                            actor: "alice".to_owned(),
                        }],
                    },
                },
                Predicate::Not {
                    predicate: Box::new(Predicate::Delta {
                        target: ValueExpr::Read {
                            method: "allowance".to_owned(),
                            args: vec![
                                ValueExpr::Actor {
                                    actor: "alice".to_owned(),
                                },
                                ValueExpr::Actor {
                                    actor: "carol".to_owned(),
                                },
                            ],
                        },
                        direction: estamora_core::DeltaDirection::Decrease,
                        by: None,
                    }),
                },
            ],
        };
        let found = reads_in_predicate(&predicate);
        let methods: Vec<&str> = found.iter().map(|(method, _)| method.as_str()).collect();
        assert_eq!(methods, vec!["allowance", "balance"]);
    }

    #[test]
    fn a_read_named_twice_is_recorded_once() {
        // Two runs of the same vector must record the same reads in the same order, or
        // the before-world differs between them and a relative requirement becomes
        // non-deterministic.
        let read = ValueExpr::Read {
            method: "balance".to_owned(),
            args: vec![ValueExpr::Actor {
                actor: "alice".to_owned(),
            }],
        };
        let predicate = Predicate::NotEqual {
            left: read.clone(),
            right: read,
        };
        assert_eq!(reads_in_predicate(&predicate).len(), 1);
    }
}
