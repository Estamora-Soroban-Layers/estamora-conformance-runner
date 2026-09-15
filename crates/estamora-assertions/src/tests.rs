//! Evaluator tests.
//!
//! Everything here runs against an in-memory world, which is the point of the
//! `World` trait: the rules about comparison are pinned without a ledger, so a
//! failure is about the rule rather than about an execution environment.
//!
//! The helpers build the environment inside the call rather than returning it,
//! because an `Environment` borrows the input map and a caller-held one would have
//! to keep a temporary alive to satisfy the borrow checker.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests may panic; a failing test is the signal"
)]

use std::collections::BTreeMap;

use estamora_core::expr::{Predicate, ValueExpr};
use estamora_core::{ErrorClass, Result};

use crate::eval::{
    Environment, Evaluation, evaluate_predicate, evaluate_predicate_for_member, evaluate_value,
};
use crate::value::Value;
use crate::world::World;

/// A world whose answers are declared rather than computed.
#[derive(Debug, Default, Clone)]
struct Fake {
    reads: BTreeMap<String, Value>,
    members: BTreeMap<String, Vec<Value>>,
    expiries: BTreeMap<String, u32>,
    ledger: u32,
}

impl Fake {
    /// A world holding only what a test declares.
    fn new() -> Self {
        Self {
            ledger: 1_000,
            ..Self::default()
        }
    }

    /// Declares what `method` returns when called with `args`.
    fn answering(mut self, method: &str, args: &[&str], value: Value) -> Self {
        self.reads.insert(key(method, args), value);
        self
    }

    fn with_members(mut self, resource: &str, members: &[i128]) -> Self {
        self.members.insert(
            resource.to_owned(),
            members.iter().copied().map(Value::Integer).collect(),
        );
        self
    }

    fn with_expiry(mut self, from: &str, spender: &str, ledger: u32) -> Self {
        self.expiries.insert(format!("{from}/{spender}"), ledger);
        self
    }

    fn at_ledger(mut self, ledger: u32) -> Self {
        self.ledger = ledger;
        self
    }
}

/// The key a read is recorded under.
fn key(method: &str, args: &[&str]) -> String {
    format!("{method}({})", args.join(","))
}

impl World for Fake {
    fn read(&self, method: &str, args: &[Value]) -> Result<Value> {
        let rendered: Vec<String> = args
            .iter()
            .map(|value| match value {
                Value::Address(name) => name.clone(),
                other => other.render(),
            })
            .collect();
        let lookup = format!("{method}({})", rendered.join(","));
        self.reads.get(&lookup).cloned().ok_or_else(|| {
            estamora_core::Error::new(
                ErrorClass::ExecutionError,
                format!("the test world was not told what {lookup:?} returns"),
            )
        })
    }

    fn resource_members(&self, resource: &str) -> Result<Vec<Value>> {
        self.members.get(resource).cloned().ok_or_else(|| {
            estamora_core::Error::new(
                ErrorClass::ExecutionError,
                format!("the test world holds no resource set {resource:?}"),
            )
        })
    }

    fn ledger_sequence(&self) -> u32 {
        self.ledger
    }

    fn allowance_expiry(&self, from: &str, spender: &str) -> Option<u32> {
        self.expiries.get(&format!("{from}/{spender}")).copied()
    }
}

/// Parses one predicate from YAML.
fn predicate(yaml: &str) -> Predicate {
    serde_yaml_ng::from_str(yaml).unwrap()
}

/// Parses one value expression from YAML.
fn expression(yaml: &str) -> ValueExpr {
    serde_yaml_ng::from_str(yaml).unwrap()
}

/// Evaluates a predicate over two worlds with no inputs.
fn evaluate(before: &Fake, after: &Fake, document: &str) -> Result<Evaluation> {
    let inputs = BTreeMap::new();
    evaluate_with(before, after, &inputs, document)
}

/// Evaluates a predicate over two worlds with declared inputs.
fn evaluate_with(
    before: &Fake,
    after: &Fake,
    inputs: &BTreeMap<String, ValueExpr>,
    document: &str,
) -> Result<Evaluation> {
    let environment = Environment {
        before,
        after,
        inputs,
        ledger_sequence: after.ledger,
    };
    evaluate_predicate(&environment, &predicate(document))
}

/// Evaluates a predicate for one resource-set member.
fn evaluate_member(world: &Fake, member: &Value, document: &str) -> Result<Evaluation> {
    let inputs = BTreeMap::new();
    let environment = Environment {
        before: world,
        after: world,
        inputs: &inputs,
        ledger_sequence: world.ledger,
    };
    evaluate_predicate_for_member(&environment, &predicate(document), member)
}

/// Computes a value expression against one world with no inputs.
fn compute(world: &Fake, document: &str) -> Result<Value> {
    let inputs = BTreeMap::new();
    compute_with(world, &inputs, document)
}

/// Computes a value expression against one world with declared inputs.
fn compute_with(
    world: &Fake,
    inputs: &BTreeMap<String, ValueExpr>,
    document: &str,
) -> Result<Value> {
    let environment = Environment {
        before: world,
        after: world,
        inputs,
        ledger_sequence: world.ledger,
    };
    evaluate_value(&environment, &expression(document))
}

/// An input map holding one actor.
fn actor_input(name: &str, actor: &str) -> BTreeMap<String, ValueExpr> {
    let mut inputs = BTreeMap::new();
    inputs.insert(
        name.to_owned(),
        expression(&format!("kind: actor\nref: {actor}")),
    );
    inputs
}

const BALANCE_OF_ALICE: &str = "{kind: read, method: balance, args: [{kind: actor, ref: alice}]}";

#[test]
fn an_amount_written_as_text_compares_with_the_integer_the_contract_returned() {
    let world = Fake::new().answering("balance", &["alice"], Value::Integer(750));

    let result = evaluate(
        &world,
        &world,
        &format!("kind: equal\nleft: {BALANCE_OF_ALICE}\nright: {{kind: literal, value: \"750\"}}"),
    )
    .unwrap();

    // The reason `Value` carries a comparison rather than an equality: a vector
    // writes an amount as a string, a contract returns an integer, and both are
    // describing the same number.
    assert!(result.held, "{result:?}");
    assert_eq!(result.expected, "750 (equal)");
    assert_eq!(result.observed, "750");
}

#[test]
fn a_failed_comparison_renders_both_sides() {
    let world = Fake::new().answering("balance", &["alice"], Value::Integer(500));

    let result = evaluate(
        &world,
        &world,
        &format!("kind: equal\nleft: {BALANCE_OF_ALICE}\nright: {{kind: literal, value: \"750\"}}"),
    )
    .unwrap();

    assert!(!result.held);
    assert_eq!(result.expected, "750 (observed is less)");
    assert_eq!(result.observed, "500");
}

#[test]
fn the_ordering_comparisons_each_hold_for_exactly_one_relation() {
    let world = Fake::new().answering("balance", &["alice"], Value::Integer(500));

    for (kind, held) in [
        ("less_than", true),
        ("less_or_equal", true),
        ("greater_than", false),
        ("greater_or_equal", false),
        ("equal", false),
        ("not_equal", true),
    ] {
        let result = evaluate(
            &world,
            &world,
            &format!(
                "kind: {kind}\nleft: {BALANCE_OF_ALICE}\nright: {{kind: literal, value: \"750\"}}"
            ),
        )
        .unwrap();
        assert_eq!(result.held, held, "{kind} of 500 and 750");
    }
}

#[test]
fn a_relative_requirement_reads_the_before_world_and_not_the_after_one() {
    // The test that catches the mistake worth making here: an implementation that
    // evaluated the target against the *after* world would compare the observed
    // value with itself, and every `unchanged` assertion would pass.
    let before = Fake::new().answering("balance", &["alice"], Value::Integer(1_000));
    let after = Fake::new().answering("balance", &["alice"], Value::Integer(750));

    let unchanged = evaluate(
        &before,
        &after,
        &format!("kind: unchanged\ntarget: {BALANCE_OF_ALICE}"),
    )
    .unwrap();
    assert!(!unchanged.held, "a changed balance is not unchanged");
    assert_eq!(unchanged.observed, "1000 → 750");

    let decreased = evaluate(
        &before,
        &after,
        &format!(
            "kind: delta\ntarget: {BALANCE_OF_ALICE}\ndirection: decrease\nby: {{kind: literal, value: \"250\"}}"
        ),
    )
    .unwrap();
    assert!(decreased.held, "{decreased:?}");
    assert_eq!(decreased.expected, "decrease by 250");
    assert_eq!(decreased.observed, "1000 → 750");
}

#[test]
fn a_relative_requirement_reports_the_movement_it_observed() {
    let before = Fake::new().answering("balance", &["alice"], Value::Integer(1_000));
    let after = Fake::new().answering("balance", &["alice"], Value::Integer(900));

    let result = evaluate(
        &before,
        &after,
        &format!(
            "kind: delta\ntarget: {BALANCE_OF_ALICE}\ndirection: decrease\nby: {{kind: literal, value: \"250\"}}"
        ),
    )
    .unwrap();

    assert!(!result.held);
    assert_eq!(result.expected, "decrease by 250 (observed -100)");
}

#[test]
fn a_stated_magnitude_of_zero_is_satisfied_by_a_value_that_did_not_move() {
    // Found by running the real corpus against the conforming fixture: a transfer of
    // zero satisfies "the sender's balance decreases by the amount", because the
    // amount is zero and the balance did change by minus zero. Testing the direction
    // separately from the magnitude rejected that, and made a contract that behaves
    // exactly as the profile requires fail a requirement it satisfies.
    let before = Fake::new().answering("balance", &["alice"], Value::Integer(1_000));
    let after = Fake::new().answering("balance", &["alice"], Value::Integer(1_000));

    let decreased = evaluate(
        &before,
        &after,
        &format!(
            "kind: delta\ntarget: {BALANCE_OF_ALICE}\ndirection: decrease\nby: {{kind: literal, value: \"0\"}}"
        ),
    )
    .unwrap();
    assert!(decreased.held, "{decreased:?}");
    assert_eq!(decreased.expected, "decrease by 0");

    // A magnitude that is not zero still requires the movement to have happened, and
    // in the stated direction: the two are one requirement, not two.
    let unchanged = evaluate(
        &before,
        &after,
        &format!(
            "kind: delta\ntarget: {BALANCE_OF_ALICE}\ndirection: decrease\nby: {{kind: literal, value: \"1\"}}"
        ),
    )
    .unwrap();
    assert!(
        !unchanged.held,
        "a balance that did not move has not decreased"
    );

    // The direction is not merely a sign convention on the magnitude: a movement of
    // 250 in the wrong direction must not satisfy either requirement.
    let rose = after.answering("balance", &["alice"], Value::Integer(1_250));
    let decrease_required = evaluate(
        &before,
        &rose,
        &format!(
            "kind: delta\ntarget: {BALANCE_OF_ALICE}\ndirection: decrease\nby: {{kind: literal, value: \"250\"}}"
        ),
    )
    .unwrap();
    assert!(
        !decrease_required.held,
        "a balance that rose by 250 did not decrease by 250"
    );

    let increase_required = evaluate(
        &before,
        &rose,
        &format!(
            "kind: delta\ntarget: {BALANCE_OF_ALICE}\ndirection: increase\nby: {{kind: literal, value: \"100\"}}"
        ),
    )
    .unwrap();
    assert!(!increase_required.held, "250 is not 100");
    assert_eq!(increase_required.expected, "increase by 100 (observed 250)");
}

#[test]
fn an_input_resolves_to_the_argument_the_operation_was_called_with() {
    let world = Fake::new().answering("balance", &["alice"], Value::Integer(1_000));
    let inputs = actor_input("who", "alice");

    let resolved = compute_with(&world, &inputs, "kind: input\nname: who").unwrap();
    assert_eq!(resolved, Value::Address("alice".to_owned()));
}

#[test]
fn a_deeply_nested_input_is_refused_rather_than_resolved_forever() {
    // A cycle written by an untrusted document. The algebra has no iteration, so a
    // cycle can only come from inputs defined in terms of each other.
    let world = Fake::new();
    let mut inputs = BTreeMap::new();
    inputs.insert("a".to_owned(), expression("kind: input\nname: b"));
    inputs.insert("b".to_owned(), expression("kind: input\nname: a"));

    let failure = compute_with(&world, &inputs, "kind: input\nname: a").unwrap_err();
    assert_eq!(failure.class(), ErrorClass::VectorError);
    assert!(
        failure.message().contains("nested it more than"),
        "{failure}"
    );
}

#[test]
fn an_undeclared_input_is_a_document_defect_rather_than_a_contract_finding() {
    let world = Fake::new();
    let failure = compute(&world, "kind: input\nname: missing").unwrap_err();
    assert_eq!(failure.class(), ErrorClass::VectorError);
}

#[test]
fn a_sum_ranges_over_the_whole_resource_set() {
    let world = Fake::new().with_members("balances", &[750, 750, 0]);

    let total = compute(&world, "kind: sum\nover: balances").unwrap();
    // Summed over the set rather than over the two accounts the operation named,
    // which is what catches a contract that credits a third account.
    assert_eq!(total, Value::Integer(1_500));
}

#[test]
fn a_resource_member_outside_an_invariant_is_refused() {
    let world = Fake::new();
    let failure = compute(&world, "kind: resource_member").unwrap_err();
    assert_eq!(failure.class(), ErrorClass::VectorError);
    assert!(failure.message().contains("invariant"));
}

#[test]
fn an_invariant_ranging_over_a_set_is_evaluated_once_per_member() {
    let world = Fake::new().with_members("balances", &[1_000, 0]);
    let document = "kind: greater_or_equal\nleft: {kind: resource_member}\nright: {kind: literal, value: \"0\"}";

    for member in world.resource_members("balances").unwrap() {
        let result = evaluate_member(&world, &member, document).unwrap();
        assert!(result.held, "{member:?}");
    }

    let result = evaluate_member(&world, &Value::Integer(-1), document).unwrap();
    assert!(!result.held, "a negative balance must violate the bound");
}

#[test]
fn an_expired_allowance_is_answered_from_the_fixture() {
    let world = Fake::new()
        .with_expiry("alice", "carol", 5_000)
        .at_ledger(6_000);

    let expiry = compute(
        &world,
        "kind: allowance_expiry\nfrom: {kind: actor, ref: alice}\nspender: {kind: actor, ref: carol}",
    )
    .unwrap();
    assert_eq!(expiry, Value::Integer(5_000));

    // The precondition a vector about expiry is written with, and the reason expiry
    // is answered from the fixture: the interface cannot report a lapsed allowance,
    // because it is indistinguishable from one that never existed.
    let expired = evaluate(
        &world,
        &world,
        "kind: less_than\nleft: {kind: allowance_expiry, from: {kind: actor, ref: alice}, spender: {kind: actor, ref: carol}}\nright: {kind: ledger_sequence}",
    )
    .unwrap();
    assert!(expired.held, "{expired:?}");

    let undeclared = compute(
        &world,
        "kind: allowance_expiry\nfrom: {kind: actor, ref: alice}\nspender: {kind: actor, ref: bob}",
    )
    .unwrap();
    assert!(undeclared.is_absent());
}

#[test]
fn the_ledger_sequence_is_the_one_the_operation_ran_at() {
    let world = Fake::new().at_ledger(4_242);
    assert_eq!(
        compute(&world, "kind: ledger_sequence").unwrap(),
        Value::Integer(4_242)
    );
}

#[test]
fn one_of_and_in_range_accept_the_readings_the_specification_permits() {
    let world = Fake::new().answering("decimals", &[], Value::Integer(7));

    let allowed = evaluate(
        &world,
        &world,
        "kind: one_of\nvalue: {kind: read, method: decimals, args: []}\nallowed: [{kind: literal, value: \"7\"}, {kind: literal, value: \"8\"}]",
    )
    .unwrap();
    assert!(allowed.held, "{allowed:?}");
    assert_eq!(allowed.expected, "one of 7, 8");

    let ranged = evaluate(
        &world,
        &world,
        "kind: in_range\nvalue: {kind: read, method: decimals, args: []}\nmin: {kind: literal, value: \"0\"}\nmax: {kind: literal, value: \"7\"}",
    )
    .unwrap();
    assert!(ranged.held, "{ranged:?}");
}

#[test]
fn a_value_the_requirement_cannot_be_applied_to_is_reported_as_incomparable() {
    // A contract that answered with a boolean where the requirement named a
    // quantity has not produced a wrong number; it has produced something the
    // requirement cannot be applied to at all. The comparison says so rather than
    // coercing the value into the shape it was hoping for.
    let world = Fake::new().answering("initialized", &[], Value::Bool(true));

    let result = evaluate(
        &world,
        &world,
        "kind: greater_than\nleft: {kind: read, method: initialized, args: []}\nright: {kind: literal, value: \"0\"}",
    )
    .unwrap();
    assert!(!result.held);
    assert_eq!(result.expected, "0 (not comparable to the observed value)");
    assert_eq!(result.observed, "true");
}

#[test]
fn text_is_ordered_as_text_because_that_is_the_only_total_order_it_has() {
    // Recorded as a decision rather than left implicit. A symbol or a name is text,
    // and a requirement about one can only order it lexically; the alternative would
    // be for every comparison against text to be incomparable, which would make
    // `equal` unusable for exactly the values it most often guards.
    let world = Fake::new().answering("symbol", &[], Value::Text("XLM".to_owned()));

    let equal = evaluate(
        &world,
        &world,
        "kind: equal\nleft: {kind: read, method: symbol, args: []}\nright: {kind: literal, value: XLM}",
    )
    .unwrap();
    assert!(equal.held, "{equal:?}");

    let greater = evaluate(
        &world,
        &world,
        "kind: greater_than\nleft: {kind: read, method: symbol, args: []}\nright: {kind: literal, value: ABC}",
    )
    .unwrap();
    assert!(greater.held, "{greater:?}");

    let lesser = evaluate(
        &world,
        &world,
        "kind: less_than\nleft: {kind: read, method: symbol, args: []}\nright: {kind: literal, value: ZZZ}",
    )
    .unwrap();
    assert!(lesser.held, "{lesser:?}");
}

#[test]
fn a_field_is_read_from_a_recorded_payload() {
    let mut payload = BTreeMap::new();
    payload.insert("amount".to_owned(), Value::Integer(250));
    let world = Fake::new().answering("event_payload", &[], Value::Record(payload));

    let field = compute(
        &world,
        "kind: field\nof: {kind: read, method: event_payload, args: []}\nfield: amount",
    )
    .unwrap();
    assert_eq!(field, Value::Integer(250));

    let missing = compute(
        &world,
        "kind: field\nof: {kind: read, method: event_payload, args: []}\nfield: recipient",
    )
    .unwrap_err();
    assert_eq!(missing.class(), ErrorClass::ExecutionError);
}

#[test]
fn arithmetic_that_leaves_the_representable_range_is_not_a_contract_finding() {
    let world = Fake::new();

    let overflow = compute(
        &world,
        "kind: arithmetic\nop: \"*\"\noperands: [{kind: literal, value: \"170141183460469231731687303715884105727\"}, {kind: literal, value: \"2\"}]",
    )
    .unwrap_err();
    assert_eq!(overflow.class(), ErrorClass::ExecutionError);

    let division = compute(
        &world,
        "kind: arithmetic\nop: \"/\"\noperands: [{kind: literal, value: \"10\"}, {kind: literal, value: \"0\"}]",
    )
    .unwrap_err();
    assert_eq!(division.class(), ErrorClass::ExecutionError);
}

#[test]
fn arithmetic_composes_so_a_derived_expectation_can_be_written() {
    let world = Fake::new().answering("balance", &["alice"], Value::Integer(1_000));
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "amount".to_owned(),
        expression("kind: literal\nvalue: \"250\""),
    );

    let result = evaluate_with(
        &world,
        &world,
        &inputs,
        &format!(
            "kind: equal\nleft: {{kind: arithmetic, op: \"-\", operands: [{BALANCE_OF_ALICE}, {{kind: input, name: amount}}]}}\nright: {{kind: literal, value: \"750\"}}"
        ),
    )
    .unwrap();
    assert!(result.held, "{result:?}");
}

#[test]
fn a_read_that_cannot_be_performed_is_an_execution_failure_not_a_failed_requirement() {
    let world = Fake::new();

    let failure = compute(&world, "kind: read\nmethod: balance\nargs: []").unwrap_err();
    // The distinction the whole error taxonomy exists for: the runner could not
    // produce an observation, so it has nothing to say about the contract.
    assert_eq!(failure.class(), ErrorClass::ExecutionError);
    assert!(!failure.blames_contract());
}

#[test]
fn canonical_integers_are_read_exactly_and_other_spellings_are_text() {
    let cases: [(&str, Option<i128>); 8] = [
        ("0", Some(0)),
        ("250", Some(250)),
        ("-250", Some(-250)),
        ("0250", None),
        ("-0", None),
        ("", None),
        ("2.5", None),
        ("alice", None),
    ];
    for (text, expected) in cases {
        assert_eq!(
            Value::integer_from_text(text),
            expected,
            "{text:?} should read as {expected:?}"
        );
    }
}

#[test]
fn composites_nest() {
    let world = Fake::new().answering("balance", &["alice"], Value::Integer(1_000));

    let result = evaluate(
        &world,
        &world,
        &format!(
            "kind: all_of\npredicates:\n  - kind: greater_than\n    left: {BALANCE_OF_ALICE}\n    right: {{kind: literal, value: \"0\"}}\n  - kind: not\n    predicate: {{kind: equal, left: {BALANCE_OF_ALICE}, right: {{kind: literal, value: \"0\"}}}}"
        ),
    )
    .unwrap();
    assert!(result.held, "{result:?}");
    assert!(result.expected.starts_with("all of ["), "{result:?}");
}
