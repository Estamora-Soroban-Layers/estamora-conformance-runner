//! What a fragment of a profile is allowed to do.
//!
//! A profile arrives as YAML documents and is parsed into typed structures before
//! anything evaluates it. That parse is the boundary this target covers, and the
//! property is not "it rejects bad input" — it is that for *every* input it either
//! produces a usable structure or a diagnostic, and never something in between.
//!
//! What must hold for every input, and therefore what this target is looking for:
//!
//! * it terminates — no input may make a parser loop;
//! * it does not allocate without bound;
//! * it does not panic, index out of range, or unwrap;
//! * a parsed requirement has the identity the evaluators assume it has. An empty or
//!   missing id would make a cross-reference unresolvable, and an unresolvable
//!   reference is a requirement that is declared, appears in `estamora profile`
//!   output, and is never evaluated against anything.
//!
//! The failure mode being guarded against is not a crash in a parser. It is a bundle
//! that reaches the execution engine having been read as something other than what it
//! said, because a requirement that was silently dropped or reinterpreted produces a
//! verdict that looks exactly like a real one.
//!
//! # What is deliberately not here
//!
//! The `id@version` → directory property, and the rule that a manifest entry may not
//! escape the bundle, both need a real directory tree to be meaningful. `ProfileReference::parse`
//! is lenient by design — it splits on `@` and refuses an empty side — so asserting a
//! grammar against it here would test an assertion nobody relies on, and the path
//! resolution that *is* relied on is covered by the loader's own tests against a
//! temporary bundle. A fuzz target that re-tests the wrong function is worse than a
//! smaller one that tests the right one.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // A profile is YAML, so anything that is not UTF-8 cannot be one. Rejecting it here
    // rather than converting lossily is deliberate: a lossy read is how two different
    // documents become the same document.
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    // The entry point: the profile's own identity, status and provenance.
    let _ = serde_yaml_ng::from_str::<estamora_profile::ProfileDocument>(text);

    // The manifest, which decides which files the bundle is made of.
    let _ = serde_yaml_ng::from_str::<estamora_profile::BundleManifest>(text);

    // The interface. Every method's arguments and return type are parsed here, and a
    // type expression is the part of a profile with the most structure to get wrong.
    if let Ok(document) = serde_yaml_ng::from_str::<estamora_profile::MethodsDocument>(text) {
        for method in &document.methods {
            assert!(!method.id.is_empty(), "a method parsed with an empty id");
            assert!(!method.name.is_empty(), "a method parsed with an empty name");
        }
    }

    // The authorization rules, whose actor and coverage are the two fields a profile
    // most often gets wrong, and which are cross-referenced by every method.
    if let Ok(document) = serde_yaml_ng::from_str::<estamora_profile::AuthorizationDocument>(text)
    {
        for rule in &document.authorization_rules {
            assert!(!rule.id.is_empty(), "an authorization rule parsed with an empty id");
        }
    }

    // The events. A definition's cardinality is a pair of integers the evaluator
    // compares against a count, so a bound that parsed into something surprising would
    // be a check that is quietly always true or always false.
    if let Ok(document) = serde_yaml_ng::from_str::<estamora_profile::EventsDocument>(text) {
        for event in &document.events {
            assert!(!event.id.is_empty(), "an event parsed with an empty id");
            assert!(!event.name.is_empty(), "an event parsed with an empty name");
        }
    }

    // The behaviour rules. A rule with neither a postcondition nor an outcome
    // expectation is a rule that cannot fail, which is worse than no rule at all.
    if let Ok(document) = serde_yaml_ng::from_str::<estamora_profile::BehaviorDocument>(text) {
        for rule in &document.behaviors {
            assert!(!rule.id.is_empty(), "a behaviour parsed with an empty id");
            assert!(!rule.method.is_empty(), "a behaviour parsed with no method");
        }
    }

    // The invariants, whose scope decides whether they are evaluated or reported as
    // inapplicable. A scope that parsed empty would make every invariant silently
    // inapplicable, which reads in a report as "nothing to check" rather than "not
    // checked".
    if let Ok(document) = serde_yaml_ng::from_str::<estamora_profile::InvariantsDocument>(text) {
        for invariant in &document.invariants {
            assert!(!invariant.id.is_empty(), "an invariant parsed with an empty id");
        }
    }

    // The failure model, whose categories are what let a refusal be classified rather
    // than merely observed.
    if let Ok(document) = serde_yaml_ng::from_str::<estamora_profile::FailuresDocument>(text) {
        for failure in &document.failures {
            assert!(!failure.id.is_empty(), "a failure parsed with an empty id");
        }
    }
});
