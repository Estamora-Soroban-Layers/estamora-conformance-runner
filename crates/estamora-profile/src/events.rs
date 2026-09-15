//! The `events.yaml` document.
//!
//! Events are part of behavioural compatibility, not a debugging convenience. An
//! indexer builds its tables from them, so a transfer that moves the right
//! balances while emitting nothing has broken every consumer downstream even
//! though the contract's own state is correct.
//!
//! # Why the model is structured rather than a list of names
//!
//! Naming the required events is not enough. A requirement has to be able to say
//! *what the event must say*: that its topics name the debited and credited
//! accounts in a fixed order, that its payload carries an amount equal to the one
//! actually moved, and that exactly one such event was emitted. So a topic and a
//! payload field each carry a [`EventBinding`], which is how a profile says "this
//! value must equal the operation's `amount` argument" instead of restating a
//! number that a different fixture would invalidate.
//!
//! # Cardinality and absence
//!
//! [`EventCardinality`] is enforced in both directions. A successful transfer
//! that emits two transfer events is a conformance failure even when both carry
//! the correct values, because a consumer cannot tell which movement is real —
//! and the same mechanism is what lets a profile require that a *refused* call
//! emits nothing at all.

use estamora_core::expr::{Literal, ValueExpr};
use serde::Deserialize;

use crate::types::{RequirementStatus, TypeExpr};

/// The `events.yaml` document.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventsDocument {
    /// The event requirements.
    pub events: Vec<EventDefinition>,
}

/// One event requirement.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventDefinition {
    /// Stable identifier, referenced by methods, vectors and ordering rules.
    pub id: String,
    /// The name the contract must use in the event's first topic.
    ///
    /// Declared separately from `id` because an implementation names an event
    /// after its own type unless it says otherwise — a Rust struct called
    /// `TransferEvent` emits `transfer_event` — so what the interface *fixes* and
    /// what the source is *called* are two different things. A profile matches
    /// the declared name, never an implementation's type name.
    pub name: String,
    /// Whether the event must, may, or must not be emitted.
    pub requirement: RequirementStatus,
    /// One sentence stating the requirement.
    pub summary: String,
    /// The full statement.
    #[serde(default)]
    pub description: Option<String>,
    /// When the event must appear relative to the operation's outcome.
    pub occurrence: EventOccurrence,
    /// The event's topics, in order.
    pub topics: Vec<EventTopic>,
    /// The event's payload.
    pub data: EventData,
    /// How many matching events the operation must emit.
    pub cardinality: EventCardinality,
    /// Ordering constraints against other events in this profile.
    pub ordering: Vec<EventOrdering>,
    /// Ids of invariants the event's values must be consistent with.
    pub correlations: Vec<String>,
    /// Anything a reader should know that is not a requirement.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// When an event must appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventOccurrence {
    /// Only when the operation succeeded.
    OnSuccess,
    /// Only when the operation failed.
    OnFailure,
    /// Whichever way the operation ended.
    Always,
}

/// One position in an event's topic list.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventTopic {
    /// Zero-based topic index.
    ///
    /// Positional, because a consumer matches on position. Bounded by the format
    /// at seven, which is where Soroban's limit on contract event topics sits.
    pub index: u32,
    /// The declared type of the value at this position.
    #[serde(rename = "type")]
    pub type_expr: TypeExpr,
    /// Where the value must come from.
    pub binding: EventBinding,
    /// What the value means.
    pub semantics: String,
}

/// The encoded shape of an event's payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventDataFormat {
    /// A single value.
    Scalar,
    /// An ordered sequence.
    Vec,
    /// Named entries.
    Map,
    /// Either the sequence or the map form.
    ///
    /// The reason this variant exists rather than a choice being made: SEP-41
    /// explicitly permits both encodings for several of its events, so a profile
    /// that picked one would fail conforming contracts that chose the other.
    Either,
}

/// An event's payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventData {
    /// The accepted encoding.
    pub format: EventDataFormat,
    /// The named entries.
    ///
    /// For the map form these are matched by name. For the sequence form the same
    /// entries are matched positionally in declaration order, which is the only
    /// reading that lets one field list describe both encodings.
    pub fields: Vec<EventDataField>,
}

/// One named entry in an event's payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventDataField {
    /// The field's name.
    pub name: String,
    /// The declared type.
    #[serde(rename = "type")]
    pub type_expr: TypeExpr,
    /// Where the value must come from.
    pub binding: EventBinding,
    /// What the value means.
    pub semantics: String,
    /// Whether the field may be absent.
    ///
    /// Explicit rather than assumed false, because a multiplexed-address payload
    /// may legitimately omit its multiplexing identifier.
    pub optional: bool,
}

/// A resource whose post-operation value an event must agree with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventDataResource {
    /// A balance.
    Balance,
    /// An allowance.
    Allowance,
    /// The total supply.
    TotalSupply,
}

/// Where an expected event value comes from.
///
/// Bindings are how a profile states a *relationship* rather than a constant. A
/// literal would tie the requirement to one fixture; a binding ties it to the
/// operation, so `transfer` moving 250 and `transfer` moving 7 are governed by
/// the same requirement.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventBinding {
    /// An exact value.
    Literal {
        /// The value.
        value: Literal,
    },
    /// A fixture actor's address.
    Actor {
        /// The actor's name.
        #[serde(rename = "ref")]
        actor: String,
    },
    /// One of the operation's arguments.
    Input {
        /// The argument's name.
        name: String,
    },
    /// The value a resource holds after the operation.
    ///
    /// This is the binding that lets a profile require that the amount stated in
    /// an event equals the amount the balances actually moved by, which is what
    /// catches a contract that emits a correct-looking event describing a
    /// different movement than the one it performed.
    StateAfter {
        /// Which resource to read.
        resource: EventDataResource,
        /// The resource's holder, as a fixture actor name.
        target: String,
    },
    /// The result of a read-only method call.
    Read {
        /// The method to call.
        method: String,
        /// The arguments, each an expression.
        args: Vec<ValueExpr>,
    },
    /// A value the profile deliberately does not constrain.
    ///
    /// Used where a specification leaves a value implementation-defined, such as
    /// the multiplexing identifier of a destination. Stating that the value is
    /// unconstrained is a decision a reviewer can see, which is the point: the
    /// alternative is silence that reads as an oversight.
    Unconstrained,
}

/// How many matching events the operation must emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventCardinality {
    /// The minimum, inclusive.
    pub min: u32,
    /// The maximum, inclusive.
    pub max: u32,
}

/// A relative ordering constraint.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventOrdering {
    /// Id of the event that must not precede this one.
    pub before: String,
    /// Whether an event of the named kind must strictly be present, rather than
    /// being allowed to be absent.
    pub strict: bool,
}
