//! What a fragment of an assertion expression is allowed to do.
//!
//! Every requirement in a profile is ultimately an expression: a predicate over a
//! resource, a value read from a contract, a comparison against a literal, an
//! arithmetic combination of the two. Profiles and vectors both contain them, and the
//! evaluator walks them recursively, which makes them the one structure in the format
//! where a malformed document can make the runner do work proportional to the document
//! rather than to the requirement.
//!
//! What must hold for every input, and therefore what this target is looking for:
//!
//! * it terminates. The algebra is a tree, and a deserialiser that accepted a hostile
//!   shape could build one deep enough to overflow the stack when the evaluator walks
//!   it. That is the failure this target exists for;
//! * it does not allocate without bound;
//! * it does not panic;
//! * a value expression has the tag it claims. `ValueExpr` is internally tagged by
//!   `kind`, so a document can express a variant that does not exist, and the
//!   deserialiser must refuse it rather than produce a default.
//!
//! Nothing here is evaluated against a contract. Evaluation takes an observation and
//! belongs in `estamora-assertions`, whose tests cover it against a fixture; what a
//! document can do *before* any contract is involved is what this boundary is.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    // A value expression: what a requirement compares against. Internally tagged, so a
    // document chooses a variant by name and a name that does not exist is a parse
    // error rather than an unrecognised value.
    let _ = serde_yaml_ng::from_str::<estamora_core::ValueExpr>(text);

    // A predicate: the shape a state assertion or an invariant takes. The one that can
    // nest furthest, because both sides of a comparison are expressions.
    let _ = serde_yaml_ng::from_str::<estamora_core::Predicate>(text);

    // A literal, which is deliberately untagged and accepts a string, a number or a
    // boolean. Everything in the format that is a quantity is written as an integer
    // string, because an `i128` does not fit a JSON number — so the interesting inputs
    // here are the ones that look like one and are not.
    let _ = serde_yaml_ng::from_str::<estamora_core::Literal>(text);

    // The arithmetic operators and the direction a relative comparison measures.
    let _ = serde_yaml_ng::from_str::<estamora_core::ArithmeticOp>(text);
    let _ = serde_yaml_ng::from_str::<estamora_core::DeltaDirection>(text);
});
