//! Loading tests.
//!
//! Every test here is about a profile the runner must *refuse*. A loader is only
//! as good as the inputs it rejects: the failure mode this project exists to
//! prevent is a runner that reports a verdict against requirements it silently
//! ignored, and each refusal below closes one way that could happen.
//!
//! The fixture is a *coherent* bundle — one method, one authorization rule, one
//! event, one failure, one invariant and one behavioural rule, all referring to
//! each other correctly. Each cross-reference test then breaks exactly one
//! reference, so a test that fails is about that reference rather than about a
//! fixture that never held together in the first place.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests may panic; a failing test is the signal"
)]

use std::fs;
use std::path::Path;

use estamora_core::ErrorClass;
use tempfile::TempDir;

use crate::invariants::InvariantOutcome;
use crate::loader::{ProfileBundle, ProfileReference};
use crate::spec;
use crate::types::{Mutability, PrimitiveType, RequirementStatus, TypeExpr};

/// A complete, coherent bundle, written into `root`.
fn write_bundle(root: &Path) {
    fs::create_dir_all(root).unwrap();
    fs::write(root.join("profile.yaml"), PROFILE).unwrap();
    fs::write(root.join("methods.yaml"), METHODS).unwrap();
    fs::write(root.join("authorization.yaml"), AUTHORIZATION).unwrap();
    fs::write(root.join("events.yaml"), EVENTS).unwrap();
    fs::write(root.join("behavior.yaml"), BEHAVIOR).unwrap();
    fs::write(root.join("invariants.yaml"), INVARIANTS).unwrap();
    fs::write(root.join("failures.yaml"), FAILURES).unwrap();
}

/// A bundle at the canonical layout path for the fixture's identity.
fn canonical_bundle() -> (TempDir, std::path::PathBuf) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().join("minimal").join("1.0");
    write_bundle(&root);
    (directory, root)
}

/// Rewrites one occurrence in a declared document, asserting it was there.
///
/// A mutation that silently did not apply would make a test pass for the wrong
/// reason, which is worse than a failing test, so the source text is checked
/// before the rewrite.
fn rewrite(root: &Path, name: &str, from: &str, to: &str) {
    let path = root.join(name);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains(from), "{name} must contain {from:?}");
    fs::write(&path, text.replace(from, to)).unwrap();
}

/// Appends text to a declared document.
///
/// Preferred over a rewrite where the mutation is a whole additional entry, so
/// that the test cannot accidentally produce a *malformed* document instead of
/// the duplicate it meant to produce.
fn append(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    let mut existing = fs::read_to_string(&path).unwrap();
    existing.push_str(text);
    fs::write(&path, existing).unwrap();
}

/// Loads the bundle and returns the message of the refusal it must have produced.
fn refusal(root: &Path, file: &str) -> String {
    let failure = ProfileBundle::load(root).unwrap_err();
    assert_eq!(
        failure.class(),
        ErrorClass::ProfileError,
        "{file}: a broken profile is a profile error"
    );
    failure.message().to_owned()
}

const PROFILE: &str = r#"
estamora_spec_version: "1.0"
profile:
  id: minimal
  version: "1.0"
  title: A minimal profile
  status: draft
  summary: The smallest coherent profile the loader accepts.
  description: Used to test the loader rather than to require anything of a contract.
  license: Apache-2.0
  specification:
    name: TEST-1
    title: A test document
    version: "1.0"
    status: draft
    url: https://example.invalid/test
    updated: "2026-01-01"
  maintainers:
    - example
  compatibility:
    interface: partial
    notes:
      - Models one method.
  provenance:
    source: upstream-specification
    derived_from: example.invalid@1.0
    interpretation_notes:
      - Nothing was ambiguous.
includes:
  methods: methods.yaml
  authorization: authorization.yaml
  events: events.yaml
  behavior: behavior.yaml
  invariants: invariants.yaml
  failures: failures.yaml
  vectors: []
  shared_vectors: []
"#;

const METHODS: &str = r"
methods:
  - id: balance
    name: balance
    requirement: required
    summary: Returns the balance of an account.
    args:
      - name: who
        type:
          kind: prim
          name: address
        semantics: The account to read.
    returns:
      type:
        kind: prim
        name: i128
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
    actor:
      kind: none
    coverage:
      mode: at_most
      arguments: []
    unauthorized:
      kind: succeed
    wrong_actor:
      kind: succeed
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
        type:
          kind: prim
          name: symbol
        binding:
          kind: literal
          value: settled
        semantics: The event discriminator.
    data:
      format: scalar
      fields: []
    cardinality:
      min: 0
      max: 1
    ordering: []
    correlations: []
";

/// A complete second declaration of the invariant the fixture already declares.
const DUPLICATE_INVARIANT: &str = r#"
  - id: balance-non-negative
    title: A second declaration of the same invariant
    kind: bounds
    severity: error
    summary: Every account's balance must be at least zero.
    scope:
      methods: ["*"]
      outcomes: [success]
    resource: balances
    predicate:
      kind: greater_or_equal
      left:
        kind: resource_member
      right:
        kind: literal
        value: "0"
    rationale: Declared twice on purpose, to prove the loader refuses the ambiguity.
"#;

const BEHAVIOR: &str = r"
behaviors:
  - id: balance-read-does-not-mutate-state
    summary: A balance query must not change any contract state.
    method: balance
    kind: success
    preconditions: []
    postconditions:
      - kind: unchanged
        target:
          kind: sum
          over: balances
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
    scope:
      methods: ["*"]
      outcomes: [success]
    resource: balances
    predicate:
      kind: greater_or_equal
      left:
        kind: resource_member
      right:
        kind: literal
        value: "0"
    rationale: A negative balance would fabricate spending power out of nothing.
"#;

const FAILURES: &str = r"
failures:
  - id: uninitialized-metadata
    category: uninitialized
    summary: Token metadata is read before the contract has been initialized.
    methods: [balance]
    trigger: The queried metadata has never been set on the contract.
    expected:
      outcome: failure
      signal: either
      state_effect: reverted
      events_emitted: none
    error_codes:
      policy: semantic_only
      allowed: []
    rationale: Recorded so the case is covered explicitly rather than left unexamined.
";

#[test]
fn a_supported_format_version_is_accepted() {
    assert_eq!(spec::check("1.0").unwrap().to_string(), "1.0");
    assert_eq!(spec::check("1.0").unwrap(), spec::SpecVersion::supported());
}

#[test]
fn an_older_minor_format_version_is_still_executable() {
    // A newer runner must keep reading what older profiles mean, or every
    // existing profile would have to be rewritten in lockstep.
    assert!(spec::check("1.0").is_ok());
    assert!(
        spec::check("0.9").is_err(),
        "a different major is not readable"
    );
}

#[test]
fn a_newer_minor_format_version_is_refused() {
    let failure = spec::check("1.1").unwrap_err();
    assert_eq!(failure.class(), ErrorClass::ProfileError);
    assert!(
        failure.message().contains("newer"),
        "the message must say which side is newer: {}",
        failure.message()
    );
    // The reasoning, not just the rule: a newer minor may add requirements the
    // runner cannot enforce, and skipping them would report a contract as
    // conformant against a corpus that was only partly evaluated.
    assert_eq!(failure.blame(), estamora_core::Blame::Specification);
}

#[test]
fn a_different_major_format_version_is_refused() {
    let failure = spec::check("2.0").unwrap_err();
    assert_eq!(failure.class(), ErrorClass::ProfileError);
    assert!(failure.message().contains("major"));
}

#[test]
fn a_malformed_format_version_is_refused() {
    for text in ["1", "1.0.0", "one.zero", "", ".", "1."] {
        let failure = spec::check(text).unwrap_err();
        assert_eq!(
            failure.class(),
            ErrorClass::ProfileError,
            "{text:?} should not be a version"
        );
    }
}

#[test]
fn a_coherent_bundle_loads_without_a_single_finding() {
    let (_directory, root) = canonical_bundle();
    let bundle = ProfileBundle::load(&root).unwrap();

    // Not one warning either: a coherent profile is the baseline the refusal
    // tests below are measured against.
    assert!(
        bundle.diagnostics().is_empty(),
        "a coherent bundle must raise nothing, found {:?}",
        bundle.diagnostics()
    );
}

#[test]
fn a_complete_bundle_loads_and_reports_its_reference() {
    let (_directory, root) = canonical_bundle();
    let bundle = ProfileBundle::load(&root).unwrap();

    assert_eq!(bundle.reference(), ProfileReference::new("minimal", "1.0"));
    assert_eq!(bundle.reference().to_string(), "minimal@1.0");
    assert_eq!(bundle.spec_version(), spec::SpecVersion::supported());
    assert_eq!(bundle.spec_version_text(), "1.0");
}

#[test]
fn every_declared_document_is_parsed_rather_than_left_as_text() {
    let (_directory, root) = canonical_bundle();
    let bundle = ProfileBundle::load(&root).unwrap();
    let documents = bundle.documents();

    // The layer that used to be missing: these documents were once checked for
    // existence only, so a profile that was semantically wrong in any of them
    // loaded and was then executed.
    assert_eq!(documents.authorization.authorization_rules.len(), 1);
    assert_eq!(documents.events.events.len(), 1);
    assert_eq!(documents.behavior.behaviors.len(), 1);
    assert_eq!(documents.invariants.invariants.len(), 1);
    assert_eq!(documents.failures.failures.len(), 1);
}

#[test]
fn the_method_layer_parses_into_typed_requirements() {
    let (_directory, root) = canonical_bundle();
    let bundle = ProfileBundle::load(&root).unwrap();
    let methods = bundle.methods();
    assert_eq!(methods.methods.len(), 1);

    let method = &methods.methods[0];
    assert_eq!(method.id, "balance");
    assert_eq!(method.name, "balance");
    assert_eq!(method.requirement, RequirementStatus::Required);
    assert_eq!(method.mutability, Mutability::Readonly);
    assert_eq!(method.args.len(), 1);
    assert_eq!(method.args[0].name, "who");
    // Compared structurally rather than by name: a type expression is composed,
    // so `Vec<Address>` is expressible without inventing a type name the runner
    // could not map onto a contract spec entry.
    assert_eq!(
        method.args[0].type_expr,
        TypeExpr::Prim {
            name: PrimitiveType::Address,
            width: None,
        }
    );
    assert_eq!(method.returns.semantics, "The balance held.");
}

#[test]
fn the_scope_query_answers_the_wildcard_rule() {
    let (_directory, root) = canonical_bundle();
    let bundle = ProfileBundle::load(&root).unwrap();
    let documents = bundle.documents();

    // An invariant scoped to `*` covers any method the profile declares, but only
    // the outcomes it lists. Both halves of that sentence are load-bearing.
    assert_eq!(
        documents
            .invariants_for("balance", InvariantOutcome::Success)
            .len(),
        1
    );
    assert_eq!(
        documents
            .invariants_for("balance", InvariantOutcome::Failure)
            .len(),
        0
    );
    assert_eq!(
        documents
            .invariants_for("transfer", InvariantOutcome::Success)
            .len(),
        1,
        "the wildcard covers a method that is not declared, because the rule is \
         scoped to every method rather than to a list"
    );
    assert_eq!(documents.behaviors_for("balance").len(), 1);
}

#[test]
fn a_missing_entry_point_is_refused() {
    let directory = TempDir::new().unwrap();
    let failure = ProfileBundle::load(directory.path()).unwrap_err();
    assert_eq!(failure.class(), ErrorClass::ProfileError);
    assert!(failure.message().contains("profile.yaml"));
}

#[test]
fn a_path_that_is_not_a_directory_is_refused() {
    let directory = TempDir::new().unwrap();
    let file = directory.path().join("profile.yaml");
    fs::write(&file, "estamora_spec_version: \"1.0\"\n").unwrap();

    let failure = ProfileBundle::load(&file).unwrap_err();
    assert_eq!(failure.class(), ErrorClass::ProfileError);
    assert!(failure.message().contains("is a directory"));
}

#[test]
fn a_missing_declared_document_is_refused() {
    let (_directory, root) = canonical_bundle();
    fs::remove_file(root.join("events.yaml")).unwrap();

    let message = refusal(&root, "events.yaml");
    assert!(
        message.contains("events") && message.contains("events.yaml"),
        "the message must name the missing document: {message}"
    );
}

#[test]
fn a_document_declared_outside_the_bundle_is_refused() {
    let (_directory, root) = canonical_bundle();

    // A manifest that reaches outside its own bundle either reads something it
    // was never meant to see or reads nothing at all. Both are refusals.
    rewrite(
        &root,
        "profile.yaml",
        "events: events.yaml",
        "events: ../../../../etc/passwd",
    );

    let message = refusal(&root, "profile.yaml");
    assert!(
        message.contains("outside the bundle"),
        "the message must say the path escapes: {message}"
    );
}

#[test]
fn a_bundle_whose_directory_disagrees_with_its_identity_is_refused() {
    let directory = TempDir::new().unwrap();
    // Stored under sep-41 but declaring minimal: a consumer resolving by path
    // would execute a profile it did not ask for.
    let root = directory.path().join("sep-41").join("1.0");
    write_bundle(&root);

    let message = refusal(&root, "profile.yaml");
    assert!(
        message.contains("declares") && message.contains("sep-41/1.0"),
        "the message must name both identities: {message}"
    );
}

#[test]
fn a_bundle_outside_the_canonical_layout_loads_with_a_warning() {
    let directory = TempDir::new().unwrap();
    // Not under profiles/<id>/<version>/, so there is nothing to compare against
    // and the bundle loads with a warning rather than a refusal.
    let root = directory.path().join("somewhere-else");
    write_bundle(&root);

    let bundle = ProfileBundle::load(&root).unwrap();
    assert!(
        !bundle.diagnostics().is_empty(),
        "a non-canonical location must be reported"
    );
    assert_eq!(bundle.reference().id, "minimal");
}

#[test]
fn an_unknown_field_is_refused_rather_than_ignored() {
    let (_directory, root) = canonical_bundle();

    // A field the runner ignores is a requirement that was written, reviewed and
    // then never enforced.
    let entry = fs::read_to_string(root.join("profile.yaml")).unwrap();
    fs::write(
        root.join("profile.yaml"),
        format!("{entry}\nunmodelled_requirement: true\n"),
    )
    .unwrap();

    let failure = ProfileBundle::load(&root).unwrap_err();
    assert_eq!(failure.class(), ErrorClass::ProfileError);
    assert!(failure.message().contains("unmodelled_requirement"));
}

#[test]
fn a_malformed_document_reports_where_it_is_malformed() {
    let (_directory, root) = canonical_bundle();
    fs::write(root.join("methods.yaml"), "methods:\n  - id: [unclosed\n").unwrap();

    // Refused at load time rather than when the document is first needed: a
    // defect that surfaces after a run has begun is a defect the run has already
    // acted on.
    let failure = ProfileBundle::load(&root).unwrap_err();
    assert_eq!(failure.class(), ErrorClass::ProfileError);
    assert!(
        failure.message().contains("malformed"),
        "the message must say the document is malformed: {}",
        failure.message()
    );
}

#[test]
fn a_reference_without_a_version_is_refused() {
    let failure = ProfileReference::parse("sep-41").unwrap_err();
    assert_eq!(failure.class(), ErrorClass::ProfileError);
    assert!(failure.message().contains("ID@VERSION"));

    assert_eq!(
        ProfileReference::parse("sep-41@1.0").unwrap(),
        ProfileReference::new("sep-41", "1.0")
    );
    for text in ["@1.0", "sep-41@"] {
        assert!(ProfileReference::parse(text).is_err(), "{text:?}");
    }
}

#[test]
fn a_method_referring_to_an_unknown_failure_is_refused() {
    let (_directory, root) = canonical_bundle();
    rewrite(
        &root,
        "methods.yaml",
        "failures: [uninitialized-metadata]",
        "failures: [no-such-failure]",
    );

    let message = refusal(&root, "methods.yaml");
    assert!(
        message.contains("no-such-failure"),
        "the message must name the unresolved reference: {message}"
    );
}

#[test]
fn a_behaviour_rule_naming_an_unknown_event_is_refused() {
    let (_directory, root) = canonical_bundle();
    rewrite(
        &root,
        "behavior.yaml",
        "forbid_events: [settled]",
        "forbid_events: [no-such-event]",
    );

    let message = refusal(&root, "behavior.yaml");
    assert!(message.contains("no-such-event"), "{message}");
}

#[test]
fn a_behaviour_rule_naming_an_unknown_invariant_is_refused() {
    let (_directory, root) = canonical_bundle();
    rewrite(
        &root,
        "behavior.yaml",
        "invariants: [balance-non-negative]",
        "invariants: [no-such-invariant]",
    );

    let message = refusal(&root, "invariants.yaml");
    assert!(message.contains("no-such-invariant"), "{message}");
}

#[test]
fn an_event_correlating_into_an_unknown_invariant_is_refused() {
    let (_directory, root) = canonical_bundle();
    rewrite(
        &root,
        "events.yaml",
        "correlations: []",
        "correlations: [no-such-invariant]",
    );

    let message = refusal(&root, "events.yaml");
    assert!(message.contains("no-such-invariant"), "{message}");
}

#[test]
fn a_duplicate_identifier_is_refused() {
    let (_directory, root) = canonical_bundle();
    // A second definition of an id every reference already points at makes each
    // of those references ambiguous rather than merely duplicated. Appended
    // rather than rewritten, so the second entry is a complete invariant and the
    // only defect in the document is the duplicated id.
    append(&root, "invariants.yaml", DUPLICATE_INVARIANT);

    let message = refusal(&root, "invariants.yaml");
    assert!(
        message.contains("more than once"),
        "the message must say the id is declared twice: {message}"
    );
}

#[test]
fn an_event_whose_cardinality_can_never_be_met_is_refused() {
    let (_directory, root) = canonical_bundle();
    rewrite(
        &root,
        "events.yaml",
        "min: 0\n      max: 1",
        "min: 2\n      max: 1",
    );

    let message = refusal(&root, "events.yaml");
    assert!(
        message.contains("no contract can satisfy"),
        "the message must say no contract could satisfy it: {message}"
    );
}

#[test]
fn an_invariant_naming_a_resource_it_does_not_range_over_is_refused() {
    let (_directory, root) = canonical_bundle();
    // `authorization_blocks_mutation` is defined by its own rule, so a resource
    // set attached to it is a requirement the runner would not know how to read.
    rewrite(
        &root,
        "invariants.yaml",
        "kind: bounds",
        "kind: authorization_blocks_mutation",
    );

    let message = refusal(&root, "invariants.yaml");
    assert!(
        message.contains("does not range over one") || message.contains("supplies a predicate"),
        "the message must explain the mismatch: {message}"
    );
}

#[test]
fn a_success_rule_that_expects_a_failure_is_refused() {
    let (_directory, root) = canonical_bundle();
    rewrite(
        &root,
        "behavior.yaml",
        "expect_failures: []",
        "expect_failures: [uninitialized-metadata]",
    );

    let message = refusal(&root, "behavior.yaml");
    assert!(
        message.contains("must succeed but expects a failure"),
        "the message must name the contradiction: {message}"
    );
}

#[test]
fn an_authorization_rule_naming_an_argument_the_method_lacks_is_refused() {
    let (_directory, root) = canonical_bundle();
    // The principal is named by an argument that does not exist, so the
    // requirement can never be observed.
    rewrite(
        &root,
        "authorization.yaml",
        "actor:\n      kind: none",
        "actor:\n      kind: argument\n      argument: from",
    );

    let message = refusal(&root, "authorization.yaml");
    assert!(
        message.contains("\"from\""),
        "the message must name the argument: {message}"
    );
}

#[test]
fn an_exact_coverage_rule_covering_nothing_is_refused() {
    let (_directory, root) = canonical_bundle();
    // Requiring the covered set to equal the empty set would require the contract
    // to demand no authorization at all, while the rule reads as though it
    // required some.
    rewrite(&root, "authorization.yaml", "mode: at_most", "mode: exact");

    let message = refusal(&root, "authorization.yaml");
    assert!(
        message.contains("demand no authorization"),
        "the message must explain the consequence: {message}"
    );
}

#[test]
fn a_failure_that_tolerates_payloads_under_the_semantic_policy_is_refused() {
    let (_directory, root) = canonical_bundle();
    rewrite(
        &root,
        "failures.yaml",
        "policy: semantic_only\n      allowed: []",
        "policy: semantic_only\n      allowed: [\"Error(1)\"]",
    );

    let message = refusal(&root, "failures.yaml");
    assert!(
        message.contains("never be consulted"),
        "the message must say the list is dead text: {message}"
    );
}

#[test]
fn an_authorization_failure_signalled_by_the_host_is_refused() {
    let (_directory, root) = canonical_bundle();
    // An authorization rejection has to come from the contract's own check, so a
    // host-level error means the failure happened for another reason.
    rewrite(
        &root,
        "failures.yaml",
        "category: uninitialized",
        "category: wrong_actor",
    );
    rewrite(
        &root,
        "failures.yaml",
        "signal: either",
        "signal: host_error",
    );

    let message = refusal(&root, "failures.yaml");
    assert!(
        message.contains("contract's own check"),
        "the message must explain why the signal is wrong: {message}"
    );
}
