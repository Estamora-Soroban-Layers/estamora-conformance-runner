//! The event dimension.
//!
//! An indexer rebuilds its tables from events. A contract that moves the right
//! balances while emitting nothing, emitting twice, or stating an amount that
//! disagrees with the movement it performed leaves every consumer downstream wrong
//! even though the contract's own storage is correct. That is why the event
//! dimension checks cardinality and payload agreement rather than presence alone.
//!
//! # Matching is by declared name, never by an implementation's type name
//!
//! It is common for an event to be named after the type that declares it — a Rust
//! struct called `TransferEvent` emits `transfer_event` unless the implementation
//! says otherwise. What the *interface* fixes is the name in the first topic, so a
//! requirement matches the profile's declared `name` and nothing else. Matching on
//! the source-language type name would make the requirement a test of one
//! implementation's naming habits.
//!
//! # Two encodings, one field list
//!
//! SEP-41 explicitly permits both a sequence and a map payload for several of its
//! events, so a profile that pinned one would reject conforming contracts that
//! chose the other. The profile's field list is therefore matched positionally
//! against a sequence and by name against a map, which is the only reading that
//! lets one list describe both.
//!
//! # Absence is checked, not assumed
//!
//! Forbidden events are enforced in the same pass as required ones. A contract
//! that emits its success event before checking a balance would otherwise look
//! conformant on a refused call, and it is precisely the contract the requirement
//! exists to catch.

use estamora_core::Result;
use estamora_profile::events::{EventDataFormat, EventDefinition};
use estamora_profile::{EventsDocument, ProfileBundle};
use estamora_vectors::{AssertionCategory, EventExpectation, VectorDocument};

use crate::eval::{Environment, evaluate_value};
use crate::observation::{ObservedCall, ObservedEvent};
use crate::outcome::AssertionOutcome;
use crate::value::Value;

/// Evaluates the event dimension for one call.
///
/// # Errors
///
/// Returns an error when an expectation names an event the profile does not
/// declare, or when a bound value could not be computed. The first is a corpus
/// defect that the vector validator should already have refused, and reporting it
/// here as an error rather than as a contract failure keeps the classification
/// honest either way.
pub fn evaluate(
    profile: &ProfileBundle,
    vector: &VectorDocument,
    environment: &Environment<'_>,
    call: &ObservedCall,
) -> Result<Vec<AssertionOutcome>> {
    let document = &profile.documents().events;
    let mut outcomes = Vec::new();

    for (position, expectation) in vector.expected.events.required.iter().enumerate() {
        outcomes.extend(required_event(
            document,
            vector,
            environment,
            call,
            expectation,
            position,
        )?);
    }

    for forbidden in &vector.expected.events.forbidden {
        outcomes.push(forbidden_event(document, vector, call, forbidden)?);
    }

    Ok(outcomes)
}

/// The cardinality and value checks for one required event.
fn required_event(
    document: &EventsDocument,
    vector: &VectorDocument,
    environment: &Environment<'_>,
    call: &ObservedCall,
    expectation: &EventExpectation,
    position: usize,
) -> Result<Vec<AssertionOutcome>> {
    let Some(definition) = find(document, &expectation.event) else {
        return Err(estamora_core::Error::new(
            estamora_core::ErrorClass::VectorError,
            format!(
                "vector `{}` requires event `{}`, which the profile does not declare",
                vector.id, expectation.event
            ),
        ));
    };

    let matched = call.events_named(&definition.name);
    let mut outcomes = Vec::new();

    // Cardinality is checked for every expectation, not only the first: two
    // expectations naming the same event are two claims about how many of it the
    // operation emits, and honouring only one of them would make the other
    // silently unenforced.
    outcomes.push(cardinality_outcome(
        definition,
        vector,
        matched.len(),
        position,
    ));

    if let Some(observed) = matched.get(position) {
        outcomes.extend(value_outcomes(
            definition,
            vector,
            environment,
            observed,
            expectation,
            position,
        )?);
    } else if matched.len() <= position {
        outcomes.push(AssertionOutcome::failed(
            format!("event/values/{}/{}", vector.id, position),
            AssertionCategory::Event,
            format!("a {} event to compare values against", definition.name),
            format!(
                "only {} such event(s) were emitted, so the required one was not available \
                 to compare",
                matched.len()
            ),
        ));
    }

    outcomes.extend(ordering_outcomes(document, definition, call));

    Ok(outcomes)
}

/// Whether the number of matching events lies within the declared bounds.
fn cardinality_outcome(
    definition: &EventDefinition,
    vector: &VectorDocument,
    count: usize,
    position: usize,
) -> AssertionOutcome {
    let count = u32::try_from(count).unwrap_or(u32::MAX);
    let held = count >= definition.cardinality.min && count <= definition.cardinality.max;
    AssertionOutcome::from_boolean(
        format!("event/cardinality/{}/{}", vector.id, position),
        AssertionCategory::Event,
        format!(
            "between {} and {} {} event(s)",
            definition.cardinality.min, definition.cardinality.max, definition.name
        ),
        format!("{count}"),
        held,
    )
    .with_detail(definition.summary.clone())
}

/// Whether an event the vector forbids was absent.
fn forbidden_event(
    document: &EventsDocument,
    vector: &VectorDocument,
    call: &ObservedCall,
    forbidden: &str,
) -> Result<AssertionOutcome> {
    let Some(definition) = find(document, forbidden) else {
        return Err(estamora_core::Error::new(
            estamora_core::ErrorClass::VectorError,
            format!(
                "vector `{}` forbids event `{forbidden}`, which the profile does not declare",
                vector.id
            ),
        ));
    };
    let matched = call.events_named(&definition.name);
    Ok(AssertionOutcome::from_boolean(
        format!("event/forbidden/{}/{}", vector.id, forbidden),
        AssertionCategory::Event,
        format!("no {} event is emitted", definition.name),
        format!("{} emitted", matched.len()),
        matched.is_empty(),
    ))
}

/// The positional comparisons of an event's topics and payload.
fn value_outcomes(
    definition: &EventDefinition,
    vector: &VectorDocument,
    environment: &Environment<'_>,
    observed: &ObservedEvent,
    expectation: &EventExpectation,
    position: usize,
) -> Result<Vec<AssertionOutcome>> {
    let mut outcomes = Vec::new();

    for (index, expected) in expectation.topics.iter().enumerate() {
        let wanted = evaluate_value(environment, expected)?;
        let found = observed.topics.get(index).cloned().unwrap_or(Value::Absent);
        outcomes.push(
            AssertionOutcome::from_boolean(
                format!("event/topic/{}/{position}/{index}", vector.id),
                AssertionCategory::Event,
                wanted.render(),
                found.render(),
                wanted.equals(&found),
            )
            .with_detail(
                definition
                    .topics
                    .iter()
                    .find(|topic| topic.index as usize == index)
                    .map_or_else(
                        || format!("topic {index}"),
                        |topic| format!("topic {index}: {}", topic.semantics),
                    ),
            ),
        );
    }

    let wanted: Vec<Value> = expectation
        .data
        .iter()
        .map(|expr| evaluate_value(environment, expr))
        .collect::<Result<Vec<Value>>>()?;

    let declared: Vec<(String, bool)> = definition
        .data
        .fields
        .iter()
        .map(|field| (field.name.clone(), field.optional))
        .collect();

    match observed.data.clone() {
        Value::Record(fields) => {
            for ((name, optional), expected) in declared.iter().zip(wanted.iter()) {
                let found = fields.get(name).cloned();
                match found {
                    Some(found) => outcomes.push(AssertionOutcome::from_boolean(
                        format!("event/data/{}/{position}/{name}", vector.id),
                        AssertionCategory::Event,
                        expected.render(),
                        found.render(),
                        expected.equals(&found),
                    )),
                    None => outcomes.push(optional_outcome(vector, position, name, *optional)),
                }
            }
        },
        Value::Sequence(items) => {
            for (index, (expected, (name, optional))) in
                wanted.iter().zip(declared.iter()).enumerate()
            {
                match items.get(index) {
                    Some(found) => outcomes.push(AssertionOutcome::from_boolean(
                        format!("event/data/{}/{position}/{name}", vector.id),
                        AssertionCategory::Event,
                        expected.render(),
                        found.render(),
                        expected.equals(found),
                    )),
                    None => outcomes.push(optional_outcome(vector, position, name, *optional)),
                }
            }
        },
        scalar => {
            // A single-value payload. The first declared field is the payload;
            // every later field must be one the profile marks optional, because
            // otherwise the contract has omitted a value the requirement states.
            if let Some((expected, (name, _))) = wanted.first().zip(declared.first()) {
                outcomes.push(AssertionOutcome::from_boolean(
                    format!("event/data/{}/{position}/{name}", vector.id),
                    AssertionCategory::Event,
                    expected.render(),
                    scalar.render(),
                    expected.equals(&scalar),
                ));
            }
            for (name, optional) in declared.iter().skip(1) {
                outcomes.push(optional_outcome(vector, position, name, *optional));
            }
        },
    }

    Ok(outcomes)
}

/// Whether the absence of a payload field is allowed.
fn optional_outcome(
    vector: &VectorDocument,
    position: usize,
    name: &str,
    optional: bool,
) -> AssertionOutcome {
    AssertionOutcome::from_boolean(
        format!("event/data/{}/{position}/{name}", vector.id),
        AssertionCategory::Event,
        format!("field `{name}` is present"),
        "absent",
        optional,
    )
}

/// The ordering constraints an event's declaration imposes.
fn ordering_outcomes(
    document: &EventsDocument,
    definition: &EventDefinition,
    call: &ObservedCall,
) -> Vec<AssertionOutcome> {
    let mut outcomes = Vec::new();
    let this_index = call
        .events
        .iter()
        .position(|event| event.name.as_deref() == Some(definition.name.as_str()));

    for ordering in &definition.ordering {
        let Some(other) = find(document, &ordering.before) else {
            continue;
        };
        let other_index = call
            .events
            .iter()
            .position(|event| event.name.as_deref() == Some(other.name.as_str()));

        let held = match (this_index, other_index) {
            // Neither emitted: nothing to order, and the presence requirement is
            // stated by cardinality rather than by ordering.
            (None, _) | (_, None) => !ordering.strict,
            (Some(this), Some(other_position)) => this < other_position,
        };

        outcomes.push(AssertionOutcome::from_boolean(
            format!("event/ordering/{}/{}", definition.id, ordering.before),
            AssertionCategory::Event,
            format!(
                "no {} event precedes the {} event{}",
                other.name,
                definition.name,
                if ordering.strict {
                    ", and one is emitted"
                } else {
                    ""
                }
            ),
            if this_index.is_some() && other_index.is_some() {
                "both emitted".to_owned()
            } else {
                "not both emitted".to_owned()
            },
            held,
        ));
    }

    outcomes
}

/// The event definition with this id.
fn find<'a>(document: &'a EventsDocument, id: &str) -> Option<&'a EventDefinition> {
    document.events.iter().find(|event| event.id == id)
}

/// Whether a behavioural rule's required event appeared.
///
/// A rule names an event by id rather than restating its topics, so this check is
/// about presence and cardinality only: what the event must *say* is stated once, in
/// `events.yaml`, and is checked by the vector that binds it to concrete values.
#[must_use]
pub fn required_by_rule(
    profile: &ProfileBundle,
    event: &str,
    owner: &str,
    call: &ObservedCall,
) -> AssertionOutcome {
    let document = &profile.documents().events;
    let Some(definition) = find(document, event) else {
        return AssertionOutcome::failed(
            format!("event/required/{owner}/{event}"),
            AssertionCategory::Event,
            format!("the profile declares an event with id `{event}`"),
            "no such event is declared",
        );
    };
    let count = u32::try_from(call.events_named(&definition.name).len()).unwrap_or(u32::MAX);
    AssertionOutcome::from_boolean(
        format!("event/required/{owner}/{event}"),
        AssertionCategory::Event,
        format!(
            "between {} and {} {} event(s)",
            definition.cardinality.min, definition.cardinality.max, definition.name
        ),
        format!("{count}"),
        count >= definition.cardinality.min && count <= definition.cardinality.max,
    )
    .with_detail(definition.summary.clone())
}

/// Whether a behavioural rule's forbidden event stayed absent.
#[must_use]
pub fn forbidden_by_rule(
    profile: &ProfileBundle,
    event: &str,
    owner: &str,
    call: &ObservedCall,
) -> AssertionOutcome {
    let document = &profile.documents().events;
    let Some(definition) = find(document, event) else {
        return AssertionOutcome::failed(
            format!("event/forbidden/{owner}/{event}"),
            AssertionCategory::Event,
            format!("the profile declares an event with id `{event}`"),
            "no such event is declared",
        );
    };
    let count = call.events_named(&definition.name).len();
    AssertionOutcome::from_boolean(
        format!("event/forbidden/{owner}/{event}"),
        AssertionCategory::Event,
        format!("no {} event is emitted", definition.name),
        format!("{count} emitted"),
        count == 0,
    )
}

/// Whether a payload shape is one the declaration permits.
///
/// Exposed for the specification-side conformance checks a report writer makes
/// when it renders which encodings were accepted.
#[must_use]
pub const fn accepts(format: EventDataFormat, observed: &Value) -> bool {
    match format {
        EventDataFormat::Scalar => matches!(
            observed,
            Value::Integer(_) | Value::Text(_) | Value::Address(_) | Value::Bool(_)
        ),
        EventDataFormat::Vec => matches!(observed, Value::Sequence(_)),
        EventDataFormat::Map => matches!(observed, Value::Record(_)),
        EventDataFormat::Either => true,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::accepts;
    use crate::value::Value;
    use estamora_profile::events::EventDataFormat;

    #[test]
    fn a_scalar_payload_is_not_a_sequence() {
        assert!(accepts(EventDataFormat::Scalar, &Value::Integer(1)));
        assert!(!accepts(
            EventDataFormat::Scalar,
            &Value::Sequence(vec![Value::Integer(1)])
        ));
    }

    #[test]
    fn either_accepts_both_encodings_sep_41_permits() {
        assert!(accepts(
            EventDataFormat::Either,
            &Value::Sequence(vec![Value::Integer(1)])
        ));
        assert!(accepts(EventDataFormat::Either, &Value::Integer(1)));
    }
}
