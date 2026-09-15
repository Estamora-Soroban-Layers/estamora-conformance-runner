//! Corpus loading tests.
//!
//! The fixture is a coherent profile bundle plus two vectors — one the bundle
//! owns, one it consumes from the shared library — so that each test below breaks
//! exactly one reference and the failure is about that reference rather than about
//! a fixture that never held together.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests may panic; a failing test is the signal"
)]

use std::fs;
use std::path::PathBuf;

use estamora_core::{ErrorClass, Result};
use estamora_profile::ProfileBundle;
use tempfile::TempDir;

use crate::loader::VectorCorpus;
use crate::model::LedgerFixture;

const PROFILE: &str = r#"
estamora_spec_version: "1.0"
profile:
  id: minimal
  version: "1.0"
  title: A minimal profile
  status: draft
  summary: The smallest coherent profile the corpus loader accepts.
  description: Used to test the vector loader rather than to require anything of a contract.
  license: Apache-2.0
  specification:
    name: TEST-1
    title: A test document
    version: "1.0"
    status: draft
    url: https://example.invalid/test
    updated: "2026-01-01"
  maintainers: [example]
  compatibility:
    interface: partial
    notes: [Models one method.]
  provenance:
    source: upstream-specification
    derived_from: example.invalid@1.0
    interpretation_notes: [Nothing was ambiguous.]
includes:
  methods: methods.yaml
  authorization: authorization.yaml
  events: events.yaml
  behavior: behavior.yaml
  invariants: invariants.yaml
  failures: failures.yaml
  vectors: [balance]
  shared_vectors: [common]
"#;

const METHODS: &str = r"
methods:
  - id: balance
    name: balance
    requirement: required
    summary: Returns the balance of an account.
    args:
      - name: who
        type: {kind: prim, name: address}
        semantics: The account to read.
    returns:
      type: {kind: prim, name: i128}
      semantics: The balance held.
    mutability: readonly
    invocation: read_only
    authorization: [balance-requires-no-authorization]
    events: [settled]
    failures: [uninitialized-metadata]
    behaviors: [balance-read-does-not-mutate-state]
";

const AUTHORIZATION: &str = r"
authorization_rules:
  - id: balance-requires-no-authorization
    summary: Reading a balance must not require authorization.
    methods: [balance]
    actor: {kind: none}
    coverage: {mode: at_most, arguments: []}
    unauthorized: {kind: succeed}
    wrong_actor: {kind: succeed}
    replay_sensitive: false
";

const EVENTS: &str = r"
events:
  - id: settled
    name: settled
    requirement: optional
    summary: An event recording that a read settled.
    occurrence: on_success
    topics:
      - index: 0
        type: {kind: prim, name: symbol}
        binding: {kind: literal, value: settled}
        semantics: The event discriminator.
    data: {format: scalar, fields: []}
    cardinality: {min: 0, max: 1}
    ordering: []
    correlations: []
";

const BEHAVIOR: &str = r"
behaviors:
  - id: balance-read-does-not-mutate-state
    summary: A balance query must not change any contract state.
    method: balance
    kind: success
    preconditions: []
    postconditions:
      - kind: unchanged
        target: {kind: sum, over: balances}
    expect_failures: []
    expect_events: []
    forbid_events: [settled]
    invariants: [balance-non-negative]
";

const INVARIANTS: &str = r#"
invariants:
  - id: balance-non-negative
    title: No balance is ever negative
    kind: bounds
    severity: error
    summary: Every account's balance must be at least zero.
    scope: {methods: ["*"], outcomes: [success]}
    resource: balances
    predicate:
      kind: greater_or_equal
      left: {kind: resource_member}
      right: {kind: literal, value: "0"}
    rationale: A negative balance would fabricate spending power out of nothing.
"#;

const FAILURES: &str = r"
failures:
  - id: uninitialized-metadata
    category: uninitialized
    summary: Token metadata is read before the contract has been initialized.
    methods: [balance]
    trigger: The queried metadata has never been set on the contract.
    expected: {outcome: failure, signal: either, state_effect: reverted, events_emitted: none}
    error_codes: {policy: semantic_only, allowed: []}
    rationale: Recorded so the case is covered explicitly rather than left unexamined.
";

/// A vector the bundle owns.
const OWNED: &str = r#"
id: balance-reports-funded-balance
profile: minimal
profile_version: "1.0"
title: A funded account reports its balance
description: Alice holds 1000 and the balance query must report exactly that value.
kind: positive
method: balance
tags: [balance, positive]
fixtures:
  actors:
    - {name: alice, kind: account}
    - {name: dave, kind: account}
  balances: {alice: "1000"}
  allowances: []
  ledger: {sequence: 1000, timestamp: "2026-01-15T12:00:00Z"}
  authorization: {alice: true, dave: true}
inputs:
  who: {kind: actor, ref: alice}
authorization: {actors: [], expected: not_required}
expected:
  outcome: success
  returns: {kind: literal, value: "1000"}
  state_assertions:
    - id: funded-account-reads-its-balance
      description: The funded account reports the balance it holds.
      resource: {kind: balance, account: alice}
      predicate:
        kind: equal
        left: {kind: read, method: balance, args: [{kind: actor, ref: alice}]}
        right: {kind: literal, value: "1000"}
  events: {required: [], forbidden: [settled]}
  invariants: [balance-non-negative]
assertions: []
rationale: Establishes that a funded account reads back the balance it holds.
references: [https://example.invalid/test]
"#;

/// A profile-independent vector the bundle consumes.
const SHARED: &str = r#"
id: unknown-account-reads-zero
profile: "*"
profile_version: "1.0"
title: An account with no entry reads as zero
description: Dave has never held a balance, and the read must report zero rather than failing.
kind: boundary
method: balance
tags: [balance, boundary]
fixtures:
  actors:
    - {name: dave, kind: account}
  balances: {}
  allowances: []
  ledger: {sequence: 1000, timestamp: "2026-01-15T12:00:00Z"}
  authorization: {dave: true}
inputs:
  who: {kind: actor, ref: dave}
authorization: {actors: [], expected: not_required}
expected:
  outcome: success
  returns: {kind: literal, value: "0"}
  state_assertions: []
  events: {required: [], forbidden: [settled]}
  invariants: []
assertions: []
rationale: A caller must be able to read a fresh account before and after funding it.
references: [https://example.invalid/test]
"#;

/// A fixture: a temp tree with a profile bundle and a shared vector library.
struct Fixture {
    #[allow(dead_code, reason = "the temp directory must outlive the fixture")]
    directory: TempDir,
    root: PathBuf,
    shared: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let root = directory.path().join("minimal").join("1.0");
        let shared = directory.path().join("shared");
        fs::create_dir_all(root.join("vectors").join("balance")).unwrap();
        fs::create_dir_all(shared.join("common").join("addresses")).unwrap();

        for (name, content) in [
            ("profile.yaml", PROFILE),
            ("methods.yaml", METHODS),
            ("authorization.yaml", AUTHORIZATION),
            ("events.yaml", EVENTS),
            ("behavior.yaml", BEHAVIOR),
            ("invariants.yaml", INVARIANTS),
            ("failures.yaml", FAILURES),
        ] {
            fs::write(root.join(name), content).unwrap();
        }
        fs::write(root.join("vectors/balance/funded.yaml"), OWNED).unwrap();
        fs::write(shared.join("common/addresses/unknown.yaml"), SHARED).unwrap();

        Self {
            directory,
            root,
            shared,
        }
    }

    /// Replaces the owned vector with `yaml`.
    fn own(&self, yaml: &str) {
        fs::write(self.root.join("vectors/balance/funded.yaml"), yaml).unwrap();
    }

    /// Replaces the shared vector with `yaml`.
    fn share(&self, yaml: &str) {
        fs::write(self.shared.join("common/addresses/unknown.yaml"), yaml).unwrap();
    }

    /// Rewrites the profile manifest.
    fn manifest(&self, from: &str, to: &str) {
        let path = self.root.join("profile.yaml");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "the manifest must contain {from:?}");
        fs::write(&path, text.replace(from, to)).unwrap();
    }

    fn load(&self) -> Result<VectorCorpus> {
        let bundle = ProfileBundle::load(&self.root)?;
        VectorCorpus::load(&bundle, &self.shared)
    }

    /// Loads and returns the message of the refusal that must have occurred.
    fn refusal(&self) -> String {
        let failure = self.load().unwrap_err();
        assert_eq!(
            failure.class(),
            ErrorClass::VectorError,
            "a broken corpus is a vector error"
        );
        failure.message().to_owned()
    }
}

/// Replaces one occurrence in a vector document, asserting it was there.
fn rewrite(yaml: &str, from: &str, to: &str) -> String {
    assert!(yaml.contains(from), "the vector must contain {from:?}");
    yaml.replace(from, to)
}

#[test]
fn a_declared_corpus_loads_both_the_owned_and_the_shared_vectors() {
    let fixture = Fixture::new();
    let corpus = fixture.load().unwrap();

    assert_eq!(corpus.len(), 2);
    assert!(corpus.by_id("balance-reports-funded-balance").is_some());
    assert!(corpus.by_id("unknown-account-reads-zero").is_some());
    // A profile that declares shared vectors consumes them: the wildcard vector
    // carries no profile reference of its own, so it is only ever discovered
    // through the declaring bundle.
    assert!(
        corpus
            .by_id("unknown-account-reads-zero")
            .unwrap()
            .is_profile_independent()
    );
    assert!(corpus.diagnostics().is_empty());
    assert!(corpus.skipped().is_empty());
}

#[test]
fn loading_the_same_tree_twice_gives_the_same_order() {
    let fixture = Fixture::new();
    let first: Vec<String> = fixture
        .load()
        .unwrap()
        .vectors()
        .iter()
        .map(|vector| vector.id().to_owned())
        .collect();
    let second: Vec<String> = fixture
        .load()
        .unwrap()
        .vectors()
        .iter()
        .map(|vector| vector.id().to_owned())
        .collect();

    // A corpus whose order depends on how the filesystem returned entries would
    // make two reports of the same run differ.
    assert_eq!(first, second);
    assert_eq!(first.len(), 2);
}

#[test]
fn a_profile_independent_vector_whose_method_is_absent_is_excluded_not_refused() {
    let fixture = Fixture::new();
    // The shared library carries a scenario for a method this profile does not
    // implement. That is not a defect in the profile: the requirement simply does
    // not apply to it.
    fixture.share(&rewrite(SHARED, "method: balance", "method: deposit"));
    fixture.manifest("shared_vectors: [common]", "shared_vectors: [common]");

    let corpus = fixture.load().unwrap();
    assert_eq!(corpus.len(), 1);
    assert_eq!(corpus.skipped().len(), 1);
    assert_eq!(corpus.skipped()[0].1, "deposit");
    assert!(
        !corpus.diagnostics().is_empty(),
        "an excluded vector must be reported rather than silently dropped"
    );
}

#[test]
fn a_vector_declaring_another_profile_is_refused() {
    let fixture = Fixture::new();
    fixture.own(&rewrite(OWNED, "profile: minimal", "profile: sep-41"));

    let message = fixture.refusal();
    assert!(
        message.contains("declares profile") && message.contains("sep-41"),
        "{message}"
    );
}

#[test]
fn a_vector_written_against_another_version_is_refused() {
    let fixture = Fixture::new();
    fixture.own(&rewrite(
        OWNED,
        "profile_version: \"1.0\"",
        "profile_version: \"2.0\"",
    ));

    let message = fixture.refusal();
    assert!(message.contains("version"), "{message}");
}

#[test]
fn an_input_the_method_does_not_take_is_refused() {
    let fixture = Fixture::new();
    fixture.own(&rewrite(
        OWNED,
        "inputs:\n  who: {kind: actor, ref: alice}",
        "inputs:\n  who: {kind: actor, ref: alice}\n  extra: {kind: literal, value: \"1\"}",
    ));

    let message = fixture.refusal();
    assert!(message.contains("\"extra\""), "{message}");
}

#[test]
fn an_argument_the_vector_does_not_supply_is_refused() {
    let fixture = Fixture::new();
    fixture.own(&rewrite(
        OWNED,
        "inputs:\n  who: {kind: actor, ref: alice}",
        "inputs: {}",
    ));

    let message = fixture.refusal();
    assert!(message.contains("\"who\""), "{message}");
}

#[test]
fn an_unknown_failure_is_refused() {
    let fixture = Fixture::new();
    fixture.own(&rewrite(
        OWNED,
        "outcome: success",
        "outcome: failure\n  failure: no-such-failure",
    ));

    let message = fixture.refusal();
    assert!(message.contains("no-such-failure"), "{message}");
}

#[test]
fn an_unknown_invariant_is_refused() {
    let fixture = Fixture::new();
    fixture.own(&rewrite(
        OWNED,
        "invariants: [balance-non-negative]",
        "invariants: [no-such-invariant]",
    ));

    let message = fixture.refusal();
    assert!(message.contains("no-such-invariant"), "{message}");
}

#[test]
fn an_unknown_event_is_refused() {
    let fixture = Fixture::new();
    fixture.own(&rewrite(
        OWNED,
        "forbidden: [settled]",
        "forbidden: [no-such-event]",
    ));

    let message = fixture.refusal();
    assert!(message.contains("no-such-event"), "{message}");
}

#[test]
fn a_reference_to_an_undeclared_actor_is_refused() {
    let fixture = Fixture::new();
    // `carol` appears in an expression but no fixture declares her, so the run
    // would be measuring a world the vector never described.
    fixture.own(&rewrite(
        OWNED,
        "args: [{kind: actor, ref: alice}]",
        "args: [{kind: actor, ref: carol}]",
    ));

    let message = fixture.refusal();
    assert!(message.contains("carol"), "{message}");
}

#[test]
fn a_read_of_an_undeclared_method_is_refused() {
    let fixture = Fixture::new();
    fixture.own(&rewrite(
        OWNED,
        "method: balance, args:",
        "method: allowance, args:",
    ));

    let message = fixture.refusal();
    assert!(message.contains("allowance"), "{message}");
}

#[test]
fn a_success_vector_that_names_a_failure_is_refused() {
    let fixture = Fixture::new();
    fixture.own(&rewrite(
        OWNED,
        "outcome: success",
        "outcome: success\n  failure: uninitialized-metadata",
    ));

    let message = fixture.refusal();
    assert!(message.contains("both outcomes at once"), "{message}");
}

#[test]
fn a_failure_vector_that_names_no_failure_is_refused() {
    let fixture = Fixture::new();
    fixture.own(&rewrite(OWNED, "outcome: success", "outcome: failure"));

    let message = fixture.refusal();
    assert!(message.contains("any failure"), "{message}");
}

#[test]
fn a_failure_vector_that_requires_an_event_is_refused() {
    let fixture = Fixture::new();
    // A refused operation must not have produced an event, so requiring one and
    // expecting a failure are contradictory.
    fixture.own(&rewrite(
        OWNED,
        "outcome: success",
        "outcome: failure\n  failure_category: invalid_argument",
    ));
    fixture.own(&rewrite(
        &fs::read_to_string(fixture.root.join("vectors/balance/funded.yaml")).unwrap(),
        "events: {required: [], forbidden: [settled]}",
        "events: {required: [{event: settled, topics: [], data: []}], forbidden: []}",
    ));

    let message = fixture.refusal();
    assert!(message.contains("must not have produced one"), "{message}");
}

#[test]
fn a_duplicate_vector_identifier_is_refused() {
    let fixture = Fixture::new();
    // The shared vector is renamed to collide with the owned one.
    fixture.share(&rewrite(
        SHARED,
        "id: unknown-account-reads-zero",
        "id: balance-reports-funded-balance",
    ));

    let message = fixture.refusal();
    assert!(
        message.contains("two vectors declare the identifier"),
        "{message}"
    );
}

#[test]
fn a_duplicate_fixture_actor_is_refused() {
    let fixture = Fixture::new();
    fixture.own(&rewrite(
        OWNED,
        "    - {name: dave, kind: account}",
        "    - {name: alice, kind: account}",
    ));

    let message = fixture.refusal();
    assert!(message.contains("more than once"), "{message}");
}

#[test]
fn a_non_canonical_amount_is_refused() {
    let fixture = Fixture::new();
    // A leading zero is a second spelling of the same value, and accepting two
    // spellings would make two otherwise identical vectors differ textually.
    fixture.own(&rewrite(OWNED, "alice: \"1000\"", "alice: \"01000\""));

    let message = fixture.refusal();
    assert!(message.contains("canonical"), "{message}");
}

#[test]
fn an_amount_written_as_a_bare_number_is_refused() {
    let fixture = Fixture::new();
    // The whole reason amounts are strings: a bare integer would reach the runner
    // as whatever the parser decided it was.
    fixture.own(&rewrite(OWNED, "alice: \"1000\"", "alice: 1000"));

    let message = fixture.refusal();
    assert!(message.contains("quoted string"), "{message}");
}

#[test]
fn a_missing_declared_vector_directory_is_refused() {
    let fixture = Fixture::new();
    fs::remove_dir_all(fixture.root.join("vectors")).unwrap();

    let message = fixture.refusal();
    assert!(message.contains("is not a directory"), "{message}");
}

#[test]
fn an_empty_declared_vector_directory_is_refused() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("vectors/balance/funded.yaml")).unwrap();

    let message = fixture.refusal();
    assert!(message.contains("holds no vectors"), "{message}");
}

#[test]
fn an_unknown_field_is_refused_rather_than_ignored() {
    let fixture = Fixture::new();
    fixture.own(&rewrite(
        OWNED,
        "rationale:",
        "unmodelled_expectation: true\nrationale:",
    ));

    let message = fixture.refusal();
    assert!(message.contains("unmodelled_expectation"), "{message}");
}

#[test]
fn tags_select_a_subset_conjunctively() {
    let fixture = Fixture::new();
    let corpus = fixture.load().unwrap();

    assert_eq!(corpus.matching(&["balance".to_owned()]).len(), 2);
    assert_eq!(
        corpus
            .matching(&["balance".to_owned(), "boundary".to_owned()])
            .len(),
        1
    );
    assert_eq!(
        corpus
            .matching(&["balance".to_owned(), "no-such-tag".to_owned()])
            .len(),
        0
    );
    assert_eq!(corpus.for_method("balance").len(), 2);
    assert_eq!(corpus.for_method("transfer").len(), 0);
}

#[test]
fn the_ledger_timestamp_parses_to_the_declared_instant() {
    let ledger = LedgerFixture {
        sequence: 1000,
        timestamp: "2026-01-15T12:00:00Z".to_owned(),
    };
    // The same instant the specification's own example vectors declare, so that a
    // fixture written from the documentation lands on the value the runner uses by
    // default.
    assert_eq!(ledger.timestamp_unix().unwrap(), 1_768_478_400);

    let malformed = LedgerFixture {
        sequence: 1000,
        timestamp: "15 January 2026".to_owned(),
    };
    let failure = malformed.timestamp_unix().unwrap_err();
    assert_eq!(failure.class(), ErrorClass::VectorError);
}
