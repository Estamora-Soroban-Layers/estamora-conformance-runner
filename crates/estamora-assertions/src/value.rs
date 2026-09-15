//! The values a requirement is evaluated against.
//!
//! An observation and an expectation rarely arrive in the same shape. A vector
//! writes an amount as a string, because that is the only way an `i128` survives
//! the document format at all; a contract returns it as an integer; a fixture
//! actor is a name in the vector and an address on the ledger. If comparison
//! demanded identical representations, every assertion would have to be written to
//! the runner's internal model rather than to the interface, and the vectors would
//! stop being documents another implementation could execute.
//!
//! So [`Value`] carries a *comparison*, not an equality, and the comparison states
//! explicitly when two values are not comparable at all. That case is reported
//! rather than coerced: a contract that returns an address where an integer was
//! required is not "not equal", it is a contract the requirement could not be
//! applied to.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

/// A value observed or expected during a run.
///
/// Addresses are carried as the **fixture actor's name**, not as a `StrKey`. That is
/// a requirement on the implementation that produces them (see
/// [`crate::world::World`]), and it is what makes an assertion readable: a
/// requirement that fails should say `alice`, not a fifty-six character address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// An exact integer.
    Integer(i128),
    /// An address, named as the vector's fixture actor names it.
    Address(String),
    /// Text.
    Text(String),
    /// A boolean.
    Bool(bool),
    /// A payload with named fields.
    Record(BTreeMap<String, Self>),
    /// An ordered payload.
    Sequence(Vec<Self>),
    /// The absence of a value.
    Absent,
}

impl Value {
    /// Reads a canonical decimal integer from text, if that is what it is.
    ///
    /// The same canonical form the document format requires, so `"0250"` is text
    /// rather than the integer 250 — two spellings of one number would make two
    /// otherwise identical requirements differ.
    #[must_use]
    pub fn integer_from_text(text: &str) -> Option<i128> {
        let digits = text.strip_prefix('-').unwrap_or(text);
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let canonical = if digits == "0" {
            !text.starts_with('-')
        } else {
            !digits.starts_with('0')
        };
        if !canonical {
            return None;
        }
        text.parse().ok()
    }

    /// This value as an integer, where that reading is unambiguous.
    #[must_use]
    pub fn as_integer(&self) -> Option<i128> {
        match self {
            Self::Integer(value) => Some(*value),
            Self::Text(text) => Self::integer_from_text(text),
            Self::Address(_)
            | Self::Bool(_)
            | Self::Record(_)
            | Self::Sequence(_)
            | Self::Absent => None,
        }
    }

    /// This value as a boolean, where that reading is unambiguous.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Text(text) => match text.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            },
            Self::Integer(_)
            | Self::Address(_)
            | Self::Record(_)
            | Self::Sequence(_)
            | Self::Absent => None,
        }
    }

    /// How this value relates to `other`.
    ///
    /// A number written as text compares as that number, because a vector must be
    /// able to say `"250"` and mean it. An address written as text compares as that
    /// text, because a vector names actors and the implementation reports them by
    /// the same name. Anything else that does not line up is
    /// [`Comparison::Incomparable`].
    #[must_use]
    pub fn compare(&self, other: &Self) -> Comparison {
        if let (Some(left), Some(right)) = (self.as_integer(), other.as_integer()) {
            return Comparison::from_ordering(left.cmp(&right));
        }
        if let (Some(left), Some(right)) = (self.as_bool(), other.as_bool()) {
            return Comparison::from_ordering(left.cmp(&right));
        }
        match (self, other) {
            // Text and addresses compare by their text. An address is named by the
            // fixture actor's name, so both sides are strings, and there is no case
            // in which one should be incomparable to the other.
            (Self::Text(left) | Self::Address(left), Self::Text(right) | Self::Address(right)) => {
                Comparison::from_ordering(left.cmp(right))
            },
            (Self::Absent, Self::Absent) => Comparison::Equal,
            _ => Comparison::Incomparable,
        }
    }

    /// Whether this value and `other` are the same value.
    #[must_use]
    pub fn equals(&self, other: &Self) -> bool {
        self.compare(other) == Comparison::Equal
    }

    /// Whether this value is absent.
    #[must_use]
    pub const fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }

    /// This value as text, for a report.
    ///
    /// Every rendering is bounded, because an observation can come from a contract
    /// and a report is a document other tools parse.
    #[must_use]
    pub fn render(&self) -> String {
        const LIMIT: usize = 320;
        let rendered = match self {
            Self::Integer(value) => value.to_string(),
            Self::Address(name) => format!("{name} (address)"),
            Self::Text(text) => text.clone(),
            Self::Bool(value) => value.to_string(),
            Self::Absent => "<absent>".to_owned(),
            Self::Sequence(items) => {
                let parts: Vec<String> = items.iter().map(Self::render).collect();
                format!("[{}]", parts.join(", "))
            },
            Self::Record(fields) => {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|(name, value)| format!("{name}: {}", value.render()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            },
        };
        if rendered.chars().count() > LIMIT {
            rendered.chars().take(LIMIT).collect()
        } else {
            rendered
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.render())
    }
}

/// How two values relate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    /// The left side is smaller.
    Less,
    /// The sides are the same value.
    Equal,
    /// The left side is larger.
    Greater,
    /// The two cannot be placed on one scale at all.
    ///
    /// Reported rather than guessed at. An implementation that produced an
    /// address where the requirement named an integer has not produced a wrong
    /// number; it has produced something the requirement cannot be applied to.
    Incomparable,
}

impl Comparison {
    /// The comparison an [`Ordering`] denotes.
    #[must_use]
    pub const fn from_ordering(ordering: Ordering) -> Self {
        match ordering {
            Ordering::Less => Self::Less,
            Ordering::Equal => Self::Equal,
            Ordering::Greater => Self::Greater,
        }
    }
}
