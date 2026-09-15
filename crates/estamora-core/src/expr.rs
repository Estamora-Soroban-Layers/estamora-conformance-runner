//! The expression algebra a specification document is written in.
//!
//! A profile and a vector both describe behaviour by composing expressions
//! rather than by embedding code. The reason is that a profile is *untrusted
//! input*: it arrives from a repository the runner does not control, is written
//! by someone the runner has no relationship with, and is then executed against a
//! contract. So a requirement must be **data to be evaluated by the runner**,
//! never a program for the runner to run.
//!
//! The algebra here is therefore total and non-Turing-complete. Every expression
//! names something the runner can already resolve — a literal, a fixture actor, a
//! declared input, a read-only method call, an aggregate over a resource set, a
//! ledger fact — and there is no recursion, no iteration and no branching beyond
//! the boolean combinators. A profile cannot express a loop because a loop is how
//! an untrusted document would consume unbounded runner resources, and it cannot
//! express a side effect because every expression is pure.
//!
//! # Why this lives in `estamora-core`
//!
//! It is the vocabulary two independent layers must agree on: `estamora-profile`
//! reads predicates out of `invariants.yaml` and `behavior.yaml`, and
//! `estamora-vectors` reads the same shapes out of a vector's expectations. A
//! second copy in either crate would be a second definition of what `equal`
//! means, and the two would drift the moment one of them gained a variant.
//!
//! # The two halves
//!
//! [`ValueExpr`] computes a value; [`Predicate`] compares computed values. Keeping
//! them separate is what makes a requirement readable: a predicate can only ever
//! be a comparison, so a profile author cannot accidentally write a requirement
//! that evaluates to a bare number and is then silently treated as truthy.

use serde::Deserialize;

/// A value written literally into a document.
///
/// Untagged, because the document format writes a literal as the value itself
/// rather than wrapping it. The variants are ordered so that a YAML scalar takes
/// its narrowest reading: a quoted `"250"` is text, a bare `250` is a number, and
/// an amount is therefore written as a string so that an `i128` cannot be rounded
/// by a parser that decided it was a double.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Literal {
    /// Text, including every integer amount.
    Text(String),
    /// A JSON number. Used for values that are genuinely fractional or that are
    /// bounded by the format, never for a token amount.
    Number(f64),
    /// A boolean.
    Bool(bool),
    /// An explicit absence.
    Null,
}

impl Literal {
    /// The literal read as text, if it is one.
    ///
    /// Most of the algebra ends up comparing text, because an amount is written
    /// as a string, so this is the accessor the evaluator reaches for most often.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Number(_) | Self::Bool(_) | Self::Null => None,
        }
    }
}

/// The arithmetic operators the algebra admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ArithmeticOp {
    /// Addition.
    #[serde(rename = "+")]
    Add,
    /// Subtraction.
    #[serde(rename = "-")]
    Subtract,
    /// Multiplication.
    #[serde(rename = "*")]
    Multiply,
    /// Division, which is exact integer division and is an error when the
    /// divisor is zero rather than an infinity.
    #[serde(rename = "/")]
    Divide,
}

/// A value computed during a run.
///
/// `deny_unknown_fields` is deliberately absent: serde cannot apply it to an
/// internally tagged enum, so an unknown *shape* is caught by the specification
/// repository's schema validation. The variants that do exist reject unknown
/// fields inside themselves, which is where a misspelled field name would land.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValueExpr {
    /// An exact value.
    Literal {
        /// The value.
        value: Literal,
    },
    /// An actor declared by the vector's fixtures.
    Actor {
        /// The fixture actor's name.
        #[serde(rename = "ref")]
        actor: String,
    },
    /// An argument of the operation under test.
    ///
    /// This is what makes a requirement survive a change of fixture: a behaviour
    /// rule can say the balance fell by *the amount argument* instead of by 250.
    Input {
        /// The argument's name, as the method declares it.
        name: String,
    },
    /// The result of calling a read-only method.
    ///
    /// Reads are how state is observed at all. The runner performs them against
    /// the same host the operation ran in, and a read that fails is an execution
    /// failure rather than a failed assertion, because a contract that cannot
    /// answer a read the profile declares has not been measured.
    Read {
        /// The method to call.
        method: String,
        /// The arguments, each of which is itself an expression.
        args: Vec<Self>,
    },
    /// The sum of every member of a resource set.
    ///
    /// Used for conservation requirements. Taking the sum over the whole set
    /// rather than over the operands of one call is what catches a contract that
    /// credits a third account nothing in the vector named.
    Sum {
        /// The resource set, e.g. `balances`.
        over: String,
    },
    /// One field of a structured value.
    Field {
        /// The value to read from.
        of: Box<Self>,
        /// The field's name.
        field: String,
    },
    /// The ledger sequence the operation executes in, as fixed by the fixture.
    ///
    /// Expiry-sensitive requirements cannot be stated without it: a profile has
    /// to be able to say that an allowance was already expired when the call was
    /// made.
    LedgerSequence,
    /// The ledger at which a fixture allowance expires.
    ///
    /// Expiry is a property the interface does not expose — once an allowance has
    /// lapsed it is indistinguishable from one that never existed — so the only
    /// way to state a requirement about it is to name the fixture that created
    /// it.
    AllowanceExpiry {
        /// Expression for the account granting the allowance.
        from: Box<Self>,
        /// Expression for the account receiving it.
        spender: Box<Self>,
    },
    /// The current member of the resource set an invariant ranges over.
    ///
    /// Only meaningful inside an invariant that declares a `resource`, which is
    /// what lets one reusable invariant state a property of *every* balance
    /// rather than of one named account.
    ResourceMember,
    /// An arithmetic combination of expressions.
    Arithmetic {
        /// The operator.
        op: ArithmeticOp,
        /// The operands, at least two.
        operands: Vec<Self>,
    },
}

/// The direction a relative comparison expects a value to move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaDirection {
    /// The value must have risen.
    Increase,
    /// The value must have fallen.
    Decrease,
    /// The value must not have moved.
    Unchanged,
}

/// A comparison evaluated against an observed execution.
///
/// Relative forms — [`Predicate::Delta`] and [`Predicate::Unchanged`] — exist so
/// that a profile can state how a value *moved* without restating its absolute
/// value. That is what keeps a vector valid across fixtures: the behaviour rule
/// says the sender's balance fell by the amount, and the same rule holds whether
/// the sender opened with 1000 or 1000000.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Predicate {
    /// Both sides are equal.
    Equal {
        /// The left side.
        left: ValueExpr,
        /// The right side.
        right: ValueExpr,
    },
    /// The sides differ.
    NotEqual {
        /// The left side.
        left: ValueExpr,
        /// The right side.
        right: ValueExpr,
    },
    /// The left side is strictly smaller.
    LessThan {
        /// The left side.
        left: ValueExpr,
        /// The right side.
        right: ValueExpr,
    },
    /// The left side is smaller or equal.
    LessOrEqual {
        /// The left side.
        left: ValueExpr,
        /// The right side.
        right: ValueExpr,
    },
    /// The left side is strictly larger.
    GreaterThan {
        /// The left side.
        left: ValueExpr,
        /// The right side.
        right: ValueExpr,
    },
    /// The left side is larger or equal.
    GreaterOrEqual {
        /// The left side.
        left: ValueExpr,
        /// The right side.
        right: ValueExpr,
    },
    /// The value equals one of the allowed values.
    ///
    /// Used where a specification permits a small enumeration of encodings, so
    /// that accepting both readings is stated rather than approximated by
    /// accepting anything.
    OneOf {
        /// The value being constrained.
        value: ValueExpr,
        /// The permitted values.
        allowed: Vec<ValueExpr>,
    },
    /// The value lies between the bounds, inclusive.
    InRange {
        /// The value being constrained.
        value: ValueExpr,
        /// The lower bound.
        min: ValueExpr,
        /// The upper bound.
        max: ValueExpr,
    },
    /// The value moved in a stated direction, optionally by an exact amount.
    Delta {
        /// The value whose movement is asserted.
        target: ValueExpr,
        /// Which way it must have moved.
        direction: DeltaDirection,
        /// The exact magnitude. Absent when the direction is `unchanged`, which
        /// the schema forbids because a value that did not move cannot have moved
        /// by an amount.
        #[serde(default)]
        by: Option<ValueExpr>,
    },
    /// The value did not move at all.
    Unchanged {
        /// The value that must not have moved.
        target: ValueExpr,
    },
    /// Every inner predicate holds. An empty list is rejected by the schema.
    AllOf {
        /// The predicates that must all hold.
        predicates: Vec<Self>,
    },
    /// At least one inner predicate holds.
    AnyOf {
        /// The predicates of which one must hold.
        predicates: Vec<Self>,
    },
    /// The inner predicate does not hold.
    Not {
        /// The negated predicate.
        predicate: Box<Self>,
    },
}

impl Predicate {
    /// The kind's name, as the document format spells it.
    ///
    /// Used by reports and diagnostics so that a failure names the comparison
    /// that failed in the vocabulary the profile was written in.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Equal { .. } => "equal",
            Self::NotEqual { .. } => "not_equal",
            Self::LessThan { .. } => "less_than",
            Self::LessOrEqual { .. } => "less_or_equal",
            Self::GreaterThan { .. } => "greater_than",
            Self::GreaterOrEqual { .. } => "greater_or_equal",
            Self::OneOf { .. } => "one_of",
            Self::InRange { .. } => "in_range",
            Self::Delta { .. } => "delta",
            Self::Unchanged { .. } => "unchanged",
            Self::AllOf { .. } => "all_of",
            Self::AnyOf { .. } => "any_of",
            Self::Not { .. } => "not",
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{DeltaDirection, Predicate, ValueExpr};

    #[test]
    fn an_amount_written_as_a_string_stays_text() {
        // The reason amounts are quoted: a bare YAML integer would be read as a
        // number, and an i128 does not survive that trip.
        let expr: ValueExpr = serde_yaml_ng::from_str("kind: literal\nvalue: \"250\"").unwrap();
        let ValueExpr::Literal { value } = expr else {
            panic!("a literal must parse as a literal");
        };
        assert_eq!(value.as_text(), Some("250"));
    }

    #[test]
    fn a_read_names_its_method_and_arguments() {
        let expr: ValueExpr = serde_yaml_ng::from_str(
            "kind: read\nmethod: balance\nargs:\n  - kind: actor\n    ref: alice\n",
        )
        .unwrap();
        let ValueExpr::Read { method, args } = &expr else {
            panic!("a read must parse as a read");
        };
        assert_eq!(method, "balance");
        assert_eq!(args.len(), 1);
        assert_eq!(
            args[0],
            ValueExpr::Actor {
                actor: "alice".to_owned()
            }
        );
    }

    #[test]
    fn a_unit_expression_carries_no_payload() {
        let expr: ValueExpr = serde_yaml_ng::from_str("kind: ledger_sequence").unwrap();
        assert_eq!(expr, ValueExpr::LedgerSequence);
    }

    #[test]
    fn the_relative_comparisons_round_trip_from_their_spelling() {
        let delta: Predicate = serde_yaml_ng::from_str(
            "kind: delta\ntarget:\n  kind: input\n  name: amount\ndirection: decrease\nby:\n  kind: input\n  name: amount\n",
        )
        .unwrap();
        let Predicate::Delta { direction, by, .. } = &delta else {
            panic!("a delta must parse as a delta");
        };
        assert_eq!(*direction, DeltaDirection::Decrease);
        assert!(by.is_some());
    }

    #[test]
    fn unchanged_needs_no_magnitude() {
        let predicate: Predicate = serde_yaml_ng::from_str(
            "kind: unchanged\ntarget:\n  kind: read\n  method: balance\n  args: []\n",
        )
        .unwrap();
        assert_eq!(predicate.kind(), "unchanged");
    }

    #[test]
    fn the_combinators_nest() {
        let predicate: Predicate = serde_yaml_ng::from_str(
            "kind: all_of\npredicates:\n  - kind: not_equal\n    left: {kind: actor, ref: a}\n    right: {kind: actor, ref: b}\n  - kind: any_of\n    predicates:\n      - kind: unchanged\n        target: {kind: ledger_sequence}\n",
        )
        .unwrap();
        assert_eq!(predicate.kind(), "all_of");
    }

    #[test]
    fn an_arithmetic_operator_reads_its_symbol() {
        let expr: ValueExpr = serde_yaml_ng::from_str(
            "kind: arithmetic\nop: \"-\"\noperands:\n  - kind: literal\n    value: \"10\"\n  - kind: input\n    name: amount\n",
        )
        .unwrap();
        let ValueExpr::Arithmetic { op, operands } = &expr else {
            panic!("an arithmetic expression must parse as one");
        };
        assert_eq!(*op, super::ArithmeticOp::Subtract);
        assert_eq!(operands.len(), 2);
    }

    #[test]
    fn every_comparison_names_itself_as_the_document_spells_it() {
        // The name is what a report quotes when a requirement fails, so a variant
        // that named itself wrongly would misattribute the failure. Each entry is
        // the document spelling paired with the predicate it must produce.
        let cases: [(&str, &str); 6] = [
            ("equal", "kind: equal"),
            ("not_equal", "kind: not_equal"),
            ("less_than", "kind: less_than"),
            ("less_or_equal", "kind: less_or_equal"),
            ("greater_than", "kind: greater_than"),
            ("greater_or_equal", "kind: greater_or_equal"),
        ];
        for (spelling, source) in cases {
            let document = format!(
                "{source}\nleft: {{kind: literal, value: \"1\"}}\nright: {{kind: literal, value: \"1\"}}\n"
            );
            let predicate: Predicate = serde_yaml_ng::from_str(&document).unwrap();
            assert_eq!(predicate.kind(), spelling);
        }

        let compound: Predicate = serde_yaml_ng::from_str(
            "kind: one_of\nvalue: {kind: literal, value: \"1\"}\nallowed: [{kind: literal, value: \"1\"}]\n",
        )
        .unwrap();
        assert_eq!(compound.kind(), "one_of");

        let range: Predicate = serde_yaml_ng::from_str(
            "kind: in_range\nvalue: {kind: literal, value: \"1\"}\nmin: {kind: literal, value: \"0\"}\nmax: {kind: literal, value: \"2\"}\n",
        )
        .unwrap();
        assert_eq!(range.kind(), "in_range");

        let negated: Predicate = serde_yaml_ng::from_str(
            "kind: not\npredicate: {kind: unchanged, target: {kind: ledger_sequence}}\n",
        )
        .unwrap();
        assert_eq!(negated.kind(), "not");
    }
}
