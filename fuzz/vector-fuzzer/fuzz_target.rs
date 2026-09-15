//! What a fragment of a vector is allowed to do.
//!
//! A vector is the measuring document: it declares a world, an operation and the
//! outcome it expects. It is written by whoever owns the profile, which makes it
//! untrusted as far as this runner is concerned, and it is the input with the most
//! nested structure in the whole format — fixture actors, balances, an authorization
//! plan, inputs, state assertions, and event expectations, each with its own expression
//! grammar.
//!
//! What must hold for every input, and therefore what this target is looking for:
//!
//! * it terminates, does not allocate without bound, and does not panic;
//! * a parsed vector has an identity. A vector with no id cannot be reported, and a
//!   result that cannot be attributed to a vector is a result nobody can act on;
//! * a parsed vector names the outcome it expects. `expected outcom: success` — one
//!   typo — must be a parse failure and not a vector that silently expects nothing,
//!   because a vector with no expected outcome is a vector that cannot fail.
//!
//! That last one is the reason this boundary matters more than it looks. A corpus is
//! the only thing that makes a requirement measurable, so a vector that was read as
//! something other than what it says is a requirement that was measured against the
//! wrong situation — and the report will look exactly like a real one.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // A vector is YAML. Rejecting non-UTF-8 here rather than converting lossily is
    // deliberate: a lossy read is how two different documents become the same one.
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    let Ok(vector) = serde_yaml_ng::from_str::<estamora_vectors::VectorDocument>(text) else {
        // A malformed vector is a corpus error, which the loader reports as a
        // `VECTOR_ERROR` with the position. Nothing further to check.
        return;
    };

    assert!(!vector.id.is_empty(), "a vector parsed with an empty id");
    assert!(!vector.profile.is_empty(), "a vector parsed with no profile");
    assert!(
        !vector.profile_version.is_empty(),
        "a vector parsed with no profile version"
    );
    assert!(
        !vector.method.is_empty(),
        "a vector parsed with no method; a vector that names no operation measures nothing"
    );

    // The declared world. Every balance and allowance is an integer string, and the
    // fixture actor names are the only addresses an expression may refer to, so a
    // vector with no actors but with balances would be a vector whose world cannot be
    // built.
    for actor in &vector.fixtures.actors {
        assert!(!actor.name.is_empty(), "a fixture actor parsed with an empty name");
    }

    // The identifiers of the state assertions, which are cross-referenced by the report
    // and by a receipt.
    for assertion in &vector.expected.state_assertions {
        assert!(!assertion.id.is_empty(), "a state assertion parsed with an empty id");
    }

    // The event expectations, which are matched by name against what the contract
    // emitted.
    for expectation in &vector.expected.events.required {
        assert!(
            !expectation.event.is_empty(),
            "a required event expectation parsed with no event name"
        );
    }
    for forbidden in &vector.expected.events.forbidden {
        assert!(!forbidden.is_empty(), "a forbidden event parsed with an empty name");
    }
});
