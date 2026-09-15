//! The interface dimension.
//!
//! Interface compatibility is the weakest of the seven claims and it is reported
//! first, because a caller reading a report needs to know that a contract exposing
//! every declared method is *still* not evidence of behavioural conformance. What
//! this dimension establishes is narrower than it looks: that the methods exist,
//! that their parameters and return types line up with what the profile declares,
//! and that a required method's absence is reported as its own finding rather than
//! as a generic failure.
//!
//! # Why inspection is a prerequisite for a negative vector
//!
//! Soroban reports a call to a method that does not exist through the same channel
//! as a deliberate `panic!()`. A contract with a missing `decimals` therefore looks
//! exactly like a contract that correctly rejects an unauthorized call. Nothing
//! about the abort itself distinguishes them, so the only way to read a refusal as
//! the contract's own decision is to have established first that the method exists
//! and is callable — which is what [`InterfaceInspection::declares`] answers, and
//! what [`observed_verifies_method`] computes.
//!
//! # An inspection that did not happen is not a failure
//!
//! When the interface could not be read — no WASM spec section, an unreadable
//! artifact — the dimension returns an error rather than a set of failed checks. A
//! contract is not non-conformant because the runner could not see it, and a run
//! that reported it so would be blaming the wrong party.

use estamora_core::{Error, ErrorClass, Result};
use estamora_profile::types::{MethodDefinition, PrimitiveType, RequirementStatus, TypeExpr};

use crate::observation::ObservedCall;
use crate::outcome::AssertionOutcome;
use estamora_vectors::AssertionCategory;

/// One parameter of an inspected method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedParameter {
    /// The parameter's name, when the inspected artifact names it.
    pub name: Option<String>,
    /// Its type, in the canonical spelling [`canonical_type`] produces.
    pub type_name: String,
}

/// One method an inspected contract exposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedMethod {
    /// The method's name.
    pub name: String,
    /// Its parameters, in order.
    pub parameters: Vec<ObservedParameter>,
    /// Its return type, canonically spelled. `None` for a void return, which is
    /// the same as `Some("void")` and is normalised here so that one spelling
    /// reaches the comparison.
    pub returns: Option<String>,
    /// Whether the method declares itself read-only.
    pub readonly: bool,
}

/// What was learned about a contract's interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterfaceInspection {
    /// The interface was read.
    Inspected {
        /// The methods the contract exposes.
        methods: Vec<ObservedMethod>,
        /// Where the interface came from, recorded so that a report says how the
        /// claim was established rather than only that it was.
        source: String,
    },
    /// The interface could not be read, with the reason.
    Unavailable {
        /// What prevented the inspection.
        detail: String,
    },
}

impl InterfaceInspection {
    /// An inspection from a list of methods and the artifact they came from.
    #[must_use]
    pub fn inspected(methods: Vec<ObservedMethod>, source: impl Into<String>) -> Self {
        Self::Inspected {
            methods,
            source: source.into(),
        }
    }

    /// Whether the interface was read at all.
    #[must_use]
    pub const fn is_inspected(&self) -> bool {
        matches!(self, Self::Inspected { .. })
    }

    /// The exposed methods, or an empty slice when nothing was read.
    #[must_use]
    pub fn methods(&self) -> &[ObservedMethod] {
        match self {
            Self::Inspected { methods, .. } => methods,
            Self::Unavailable { .. } => &[],
        }
    }

    /// The method with this name, if one was exposed.
    #[must_use]
    pub fn method(&self, name: &str) -> Option<&ObservedMethod> {
        self.methods().iter().find(|method| method.name == name)
    }

    /// Whether a method of this name is present in the inspected interface.
    ///
    /// Presence only. A signature mismatch is a conformance failure in its own
    /// right, and it does not make a refusal unreadable: a contract whose
    /// `transfer` takes the wrong parameters still refuses deliberately when its
    /// own check fails.
    #[must_use]
    pub fn declares(&self, name: &str) -> bool {
        self.is_inspected() && self.method(name).is_some()
    }
}

/// Whether a call's refusal can be attributed to the contract.
///
/// `true` only when the interface was read and declares the method that was
/// called. Every negative vector's verdict depends on this, so it is computed in
/// one place rather than decided at each call site that reads a refusal.
#[must_use]
pub fn observed_verifies_method(call: &ObservedCall, inspection: &InterfaceInspection) -> bool {
    inspection.declares(&call.method)
}

/// The canonical spelling of a declared type.
///
/// Comparison is textual and canonical rather than structural, because both sides
/// arrive as text: a contract's interface is read out of its own artifact, and the
/// profile's declaration is a document. Rendering both once through this function
/// is what makes `Vec<Address>` on one side and `vec<address>` on the other the
/// same claim rather than a formatting difference reported as a failure.
#[must_use]
pub fn canonical_type(expr: &TypeExpr) -> String {
    match expr {
        TypeExpr::Prim { name, width } => match (name, width) {
            (PrimitiveType::BytesN, Some(width)) => format!("bytes_n<{width}>"),
            _ => primitive_name(*name).to_owned(),
        },
        TypeExpr::Vec { element } => format!("vec<{}>", canonical_type(element)),
        TypeExpr::Option { some } => format!("option<{}>", canonical_type(some)),
        TypeExpr::Map { key, value } => {
            format!("map<{},{}>", canonical_type(key), canonical_type(value))
        },
        TypeExpr::Tuple { elements } => {
            let parts: Vec<String> = elements.iter().map(canonical_type).collect();
            format!("tuple<{}>", parts.join(","))
        },
        TypeExpr::Result { ok, err } => {
            format!("result<{},{}>", canonical_type(ok), canonical_type(err))
        },
        TypeExpr::Custom { name, .. } => name.to_lowercase(),
    }
}

/// The canonical name of a primitive.
///
/// `void` and `val` are normalised to one spelling each, and `bytes_n` without a
/// width is reported as `bytes_n` rather than refused: the width is a detail of
/// the declaration, not of the claim.
fn primitive_name(name: PrimitiveType) -> &'static str {
    match name {
        PrimitiveType::Address => "address",
        PrimitiveType::MuxedAddress => "muxed_address",
        PrimitiveType::I128 => "i128",
        PrimitiveType::U128 => "u128",
        PrimitiveType::I64 => "i64",
        PrimitiveType::U64 => "u64",
        PrimitiveType::I32 => "i32",
        PrimitiveType::U32 => "u32",
        PrimitiveType::Bool => "bool",
        PrimitiveType::Symbol => "symbol",
        PrimitiveType::String => "string",
        PrimitiveType::Bytes => "bytes",
        PrimitiveType::BytesN => "bytes_n",
        PrimitiveType::Void => "void",
        PrimitiveType::Val => "val",
        PrimitiveType::Timepoint => "timepoint",
        PrimitiveType::Duration => "duration",
    }
}

/// Evaluates the interface dimension.
///
/// # Errors
///
/// Returns an execution error when the interface could not be read. No statement
/// about the contract follows from an inspection that did not happen.
pub fn evaluate(
    methods: &[MethodDefinition],
    inspection: &InterfaceInspection,
) -> Result<Vec<AssertionOutcome>> {
    let source = match inspection {
        InterfaceInspection::Inspected { source, .. } => source,
        InterfaceInspection::Unavailable { detail } => {
            return Err(Error::new(
                ErrorClass::ExecutionError,
                format!("the contract interface could not be inspected: {detail}"),
            )
            .with_context("dimension", "interface"));
        },
    };

    let mut outcomes = Vec::new();
    for method in methods {
        outcomes.extend(evaluate_method(method, inspection, source));
    }
    Ok(outcomes)
}

/// The checks for one declared method.
fn evaluate_method(
    method: &MethodDefinition,
    inspection: &InterfaceInspection,
    source: &str,
) -> Vec<AssertionOutcome> {
    let presence_id = format!("interface/method/{}", method.id);
    let observed = inspection.method(&method.name);

    match (method.requirement, observed) {
        (RequirementStatus::Forbidden, Some(found)) => {
            // A forbidden method that is exposed is a failure even though the
            // call would have been the caller's to avoid: an implementation that
            // publishes a method the standard forbids has extended an interface
            // other code is entitled to rely on not being there.
            vec![AssertionOutcome::failed(
                presence_id,
                AssertionCategory::Interface,
                format!("method {} is forbidden by this profile", method.name),
                format!(
                    "exposed by {source} with {} parameter(s)",
                    found.parameters.len()
                ),
            )]
        },
        (RequirementStatus::Forbidden, None) => vec![AssertionOutcome::passed(
            presence_id,
            AssertionCategory::Interface,
            format!("method {} is forbidden by this profile", method.name),
            "not exposed",
        )],
        (RequirementStatus::Required, None) => vec![
            AssertionOutcome::failed(
                presence_id,
                AssertionCategory::Interface,
                format!("required method {} is available", method.name),
                format!("absent from {source}"),
            )
            .with_detail(
                "A missing method aborts exactly as a deliberate panic does, so every \
             refusal recorded while evaluating this profile must be read as \
             unverified."
                    .to_owned(),
            ),
        ],
        (RequirementStatus::Required, Some(found)) => {
            let mut outcomes = vec![AssertionOutcome::passed(
                presence_id,
                AssertionCategory::Interface,
                format!("required method {} is available", method.name),
                format!("present in {source}"),
            )];
            outcomes.extend(signature_checks(method, found));
            outcomes
        },
        (RequirementStatus::Optional, None) => vec![
            AssertionOutcome::passed(
                presence_id,
                AssertionCategory::Interface,
                format!("optional method {} may be absent", method.name),
                "not exposed",
            )
            .with_detail(
                "Reported as satisfied because the profile marks the method optional; the \
             report records the absence so that a reader can see which surface was \
             exercised."
                    .to_owned(),
            ),
        ],
        (RequirementStatus::Optional, Some(found)) => signature_checks(method, found),
    }
}

/// Arity, parameter-type and return-type checks for a method that is present.
fn signature_checks(method: &MethodDefinition, found: &ObservedMethod) -> Vec<AssertionOutcome> {
    let mut outcomes = Vec::new();

    let declared: Vec<String> = method
        .args
        .iter()
        .map(|argument| canonical_type(&argument.type_expr))
        .collect();
    let observed: Vec<String> = found
        .parameters
        .iter()
        .map(|parameter| parameter.type_name.clone())
        .collect();

    outcomes.push(AssertionOutcome::from_boolean(
        format!("interface/arity/{}", method.id),
        AssertionCategory::Interface,
        format!("{} parameter(s)", declared.len()),
        format!("{} parameter(s)", observed.len()),
        declared.len() == observed.len(),
    ));

    if declared.len() == observed.len() {
        for (position, (expected, actual)) in declared.iter().zip(observed.iter()).enumerate() {
            let matches = expected == actual || actual == "val";
            outcomes.push(
                AssertionOutcome::from_boolean(
                    format!("interface/parameter/{}/{}", method.id, position),
                    AssertionCategory::Interface,
                    expected.clone(),
                    actual.clone(),
                    matches,
                )
                .with_detail(match method.args.get(position) {
                    Some(argument) => format!("parameter `{}`", argument.name),
                    None => format!("parameter at position {position}"),
                }),
            );
        }
    } else {
        // A mismatch in arity is one finding rather than a cascade of positional
        // ones: reporting every later parameter as wrong because the first was
        // missing would bury the single defect that explains all of them.
        outcomes.push(AssertionOutcome::failed(
            format!("interface/parameter-types/{}", method.id),
            AssertionCategory::Interface,
            format!("parameter types [{},]", declared.join(", ")),
            "not compared: the parameter counts differ",
        ));
    }

    let declared_return = canonical_type(&method.returns.type_expr);
    let observed_return = found.returns.clone().unwrap_or_else(|| "void".to_owned());
    outcomes.push(AssertionOutcome::from_boolean(
        format!("interface/return/{}", method.id),
        AssertionCategory::Interface,
        declared_return.clone(),
        observed_return.clone(),
        declared_return == observed_return || observed_return == "val",
    ));

    outcomes
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{InterfaceInspection, ObservedMethod, ObservedParameter, canonical_type};
    use estamora_profile::types::{PrimitiveType, TypeExpr};

    fn address() -> TypeExpr {
        TypeExpr::Prim {
            name: PrimitiveType::Address,
            width: None,
        }
    }

    #[test]
    fn a_composed_type_reads_the_way_the_document_writes_it() {
        let vector = TypeExpr::Vec {
            element: Box::new(address()),
        };
        assert_eq!(canonical_type(&vector), "vec<address>");
        let map = TypeExpr::Map {
            key: Box::new(address()),
            value: Box::new(TypeExpr::Prim {
                name: PrimitiveType::I128,
                width: None,
            }),
        };
        assert_eq!(canonical_type(&map), "map<address,i128>");
    }

    #[test]
    fn an_inspection_that_did_not_happen_declares_nothing() {
        let inspection = InterfaceInspection::Unavailable {
            detail: "no spec section".to_owned(),
        };
        assert!(!inspection.is_inspected());
        assert!(!inspection.declares("transfer"));
    }

    #[test]
    fn a_declared_method_is_found_by_name_alone() {
        let inspection = InterfaceInspection::inspected(
            vec![ObservedMethod {
                name: "balance".to_owned(),
                parameters: vec![ObservedParameter {
                    name: Some("id".to_owned()),
                    type_name: "address".to_owned(),
                }],
                returns: Some("i128".to_owned()),
                readonly: true,
            }],
            "fixture",
        );
        assert!(inspection.declares("balance"));
        assert!(!inspection.declares("transfer"));
    }
}
