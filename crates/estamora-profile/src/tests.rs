//! Loading tests.
//!
//! Every test here is about a profile the runner must *refuse*. A loader is only
//! as good as the inputs it rejects: the failure mode this project exists to
//! prevent is a runner that reports a verdict against requirements it silently
//! ignored, and each refusal below closes one way that could happen.
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

use crate::loader::{ProfileBundle, ProfileReference};
use crate::spec;
use crate::types::{Mutability, PrimitiveType, RequirementStatus, TypeExpr};

/// A complete, minimal bundle, written into `root`.
///
/// The content is the smallest profile the loader will accept, so that a test
/// that fails is about the loader rather than about the fixture's completeness.
fn write_bundle(root: &Path) {
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join("profile.yaml"),
        r#"
estamora_spec_version: "1.0"
profile:
  id: minimal
  version: "1.0"
  title: A minimal profile
  status: draft
  summary: The smallest profile the loader accepts.
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
"#,
    )
    .unwrap();

    fs::write(
        root.join("methods.yaml"),
        r"
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
    authorization: []
    events: []
    failures: []
    behaviors: []
",
    )
    .unwrap();

    for name in [
        "authorization.yaml",
        "events.yaml",
        "behavior.yaml",
        "invariants.yaml",
        "failures.yaml",
    ] {
        // The loader only requires these to exist; their contents are the
        // specification layer's concern until the documents that reference them
        // are modelled.
        fs::write(root.join(name), "\n").unwrap();
    }
}

fn bundle_at_path(path: &Path) -> Result<ProfileBundle, estamora_core::Error> {
    ProfileBundle::load(path)
}

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
fn a_complete_bundle_loads_and_reports_its_reference() {
    let directory = TempDir::new().unwrap();
    let root = directory.path().join("minimal").join("1.0");
    write_bundle(&root);

    let bundle = bundle_at_path(&root).unwrap();
    assert_eq!(bundle.reference(), ProfileReference::new("minimal", "1.0"));
    assert_eq!(bundle.reference().to_string(), "minimal@1.0");
    assert_eq!(bundle.spec_version(), spec::SpecVersion::supported());
    assert_eq!(bundle.spec_version_text(), "1.0");
}

#[test]
fn the_method_layer_parses_into_typed_requirements() {
    let directory = TempDir::new().unwrap();
    let root = directory.path().join("minimal").join("1.0");
    write_bundle(&root);

    let bundle = bundle_at_path(&root).unwrap();
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
fn a_missing_entry_point_is_refused() {
    let directory = TempDir::new().unwrap();
    let failure = bundle_at_path(directory.path()).unwrap_err();
    assert_eq!(failure.class(), ErrorClass::ProfileError);
    assert!(failure.message().contains("profile.yaml"));
}

#[test]
fn a_path_that_is_not_a_directory_is_refused() {
    let directory = TempDir::new().unwrap();
    let file = directory.path().join("profile.yaml");
    fs::write(&file, "estamora_spec_version: \"1.0\"\n").unwrap();

    let failure = bundle_at_path(&file).unwrap_err();
    assert_eq!(failure.class(), ErrorClass::ProfileError);
    assert!(failure.message().contains("is a directory"));
}

#[test]
fn a_missing_declared_document_is_refused() {
    let directory = TempDir::new().unwrap();
    let root = directory.path().join("minimal").join("1.0");
    write_bundle(&root);
    fs::remove_file(root.join("events.yaml")).unwrap();

    let failure = bundle_at_path(&root).unwrap_err();
    assert_eq!(failure.class(), ErrorClass::ProfileError);
    assert!(
        failure.message().contains("events") && failure.message().contains("events.yaml"),
        "the message must name the missing document: {}",
        failure.message()
    );
}

#[test]
fn a_document_declared_outside_the_bundle_is_refused() {
    let directory = TempDir::new().unwrap();
    let root = directory.path().join("minimal").join("1.0");
    write_bundle(&root);

    // A manifest that reaches outside its own bundle either reads something it
    // was never meant to see or reads nothing at all. Both are refusals.
    let entry = fs::read_to_string(root.join("profile.yaml")).unwrap();
    fs::write(
        root.join("profile.yaml"),
        entry.replace("events: events.yaml", "events: ../../../../etc/passwd"),
    )
    .unwrap();

    let failure = bundle_at_path(&root).unwrap_err();
    assert_eq!(failure.class(), ErrorClass::ProfileError);
    assert!(
        failure.message().contains("outside the bundle"),
        "the message must say the path escapes: {}",
        failure.message()
    );
}

#[test]
fn a_bundle_whose_directory_disagrees_with_its_identity_is_refused() {
    let directory = TempDir::new().unwrap();
    // Stored under sep-41 but declaring minimal: a consumer resolving by path
    // would execute a profile it did not ask for.
    let root = directory.path().join("sep-41").join("1.0");
    write_bundle(&root);

    let failure = bundle_at_path(&root).unwrap_err();
    assert_eq!(failure.class(), ErrorClass::ProfileError);
    assert!(
        failure.message().contains("declares") && failure.message().contains("sep-41/1.0"),
        "the message must name both identities: {}",
        failure.message()
    );
}

#[test]
fn a_bundle_outside_the_canonical_layout_loads_with_a_warning() {
    let directory = TempDir::new().unwrap();
    // Not under profiles/<id>/<version>/, so there is nothing to compare against
    // and the bundle loads with a warning rather than a refusal.
    let root = directory.path().join("somewhere-else");
    write_bundle(&root);

    let bundle = bundle_at_path(&root).unwrap();
    assert!(
        !bundle.diagnostics().is_empty(),
        "a non-canonical location must be reported"
    );
    assert_eq!(bundle.reference().id, "minimal");
}

#[test]
fn an_unknown_field_is_refused_rather_than_ignored() {
    let directory = TempDir::new().unwrap();
    let root = directory.path().join("minimal").join("1.0");
    write_bundle(&root);

    // A field the runner ignores is a requirement that was written, reviewed and
    // then never enforced.
    let entry = fs::read_to_string(root.join("profile.yaml")).unwrap();
    fs::write(
        root.join("profile.yaml"),
        format!("{entry}\nunmodelled_requirement: true\n"),
    )
    .unwrap();

    let failure = bundle_at_path(&root).unwrap_err();
    assert_eq!(failure.class(), ErrorClass::ProfileError);
    assert!(failure.message().contains("unmodelled_requirement"));
}

#[test]
fn a_malformed_document_reports_where_it_is_malformed() {
    let directory = TempDir::new().unwrap();
    let root = directory.path().join("minimal").join("1.0");
    write_bundle(&root);
    fs::write(root.join("methods.yaml"), "methods:\n  - id: [unclosed\n").unwrap();

    // Refused at load time rather than when the document is first needed: a
    // defect that surfaces after a run has begun is a defect the run has already
    // acted on.
    let failure = bundle_at_path(&root).unwrap_err();
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
