//! The scaffolding the benchmarks share.
//!
//! Every benchmark here measures one of four things, and all four need the same setup:
//! the fixture bundle this repository measures its own contracts with, its corpus, and a
//! stored report to render. That setup lives here so that a benchmark body is the call
//! being measured and nothing else.
//!
//! # Where the fixtures come from
//!
//! `fixtures/` is laid out the way a specification checkout is — `profiles/<id>/<version>/`
//! — so loading it here exercises the same resolution a user's run takes, against a
//! bundle in this repository rather than against a sibling checkout. A benchmark that
//! needed `estamora-conformance-spec` would not run in CI on a checkout of this
//! repository alone, which is exactly when a regression needs to be caught.
//!
//! # A note on what is measured
//!
//! Nothing here is a microbenchmark of a function nobody calls. The interesting costs in
//! this tool are: loading a bundle and validating every cross-reference in it, which is
//! what a contributor waits for on every `estamora validate`; executing a vector, which
//! is a deployment and an invocation and not an arithmetic loop; rendering a report, which
//! is bounded by the report size; and loading a large corpus, which is the case that has
//! to stay linear. A benchmark of the expression evaluator in isolation would measure
//! nothing anybody experiences.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a benchmark harness may panic; a failed run is the signal"
)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The repository root, derived from this crate's own directory.
#[must_use]
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The tree the fixture profile bundle lives in.
#[must_use]
pub fn fixture_spec_root() -> PathBuf {
    repo_root().join("fixtures")
}

/// The fixture profile bundle's directory.
#[must_use]
pub fn fixture_profile_root() -> PathBuf {
    fixture_spec_root().join("profiles/conformance-token/1.0")
}

/// Loads the fixture bundle.
///
/// # Panics
///
/// Aborts when the bundle cannot be loaded, which would make every number below
/// meaningless rather than merely slower.
#[must_use]
pub fn fixture_bundle() -> estamora_profile::ProfileBundle {
    estamora_profile::ProfileBundle::load(fixture_profile_root())
        .expect("the fixture bundle must load")
}

/// Loads the fixture bundle and its corpus.
///
/// # Panics
///
/// As [`fixture_bundle`].
#[must_use]
pub fn fixture_corpus() -> (
    estamora_profile::ProfileBundle,
    estamora_vectors::VectorCorpus,
) {
    let bundle = fixture_bundle();
    let corpus = estamora_vectors::VectorCorpus::load(&bundle, fixture_spec_root())
        .expect("the fixture corpus must load");
    (bundle, corpus)
}

/// Reads the stored report.
///
/// Its timestamp is pinned, so the document is stable and a benchmark of rendering it
/// measures rendering rather than reading.
///
/// # Panics
///
/// Aborts when the document cannot be read or parsed, which would mean the fixture set
/// has changed shape.
#[must_use]
pub fn stored_report_text() -> String {
    let path = repo_root().join("fixtures/expected-reports/conformant-transfer.json");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|problem| panic!("{} could not be read: {problem}", path.display()))
}

/// The parsed stored report.
///
/// # Panics
///
/// As [`stored_report_text`].
#[must_use]
pub fn stored_report() -> estamora_report::Report {
    estamora_report::parse(&stored_report_text()).expect("the stored report must parse")
}

/// The configuration for a run of the fixture profile against a fixture.
///
/// # Panics
///
/// Aborts when the target cannot be named, which is a defect in the benchmark rather
/// than a result about a contract.
#[must_use]
pub fn run_config(defect: &str) -> estamora_cli::RunConfig {
    let target = estamora_cli::engine::Target::parse(&format!("fixture:{defect}"), None)
        .unwrap_or_else(|problem| panic!("`fixture:{defect}` is not a target: {problem}"));
    estamora_cli::RunConfig::new(fixture_profile_root(), fixture_spec_root(), target)
}

/// A synthetic bundle of `methods` methods and `vectors` vectors, written to disk.
///
/// Generated rather than committed because the point is to measure how loading scales
/// with size, and a committed corpus can only be one size. The bundle is a valid one —
/// every document the manifest names is present and every cross-reference resolves — so
/// the numbers measure the loader rather than its error path.
///
/// The caller owns the returned directory: dropping it removes the bundle, so a failed
/// benchmark cannot leave one behind.
///
/// # Panics
///
/// Aborts when the tree cannot be written, which is a defect in the benchmark.
#[must_use]
pub fn generate_bundle(methods: usize, vectors: usize) -> tempfile::TempDir {
    let directory = tempfile::TempDir::new().expect("a temporary directory must be creatable");
    let root = directory.path().join("bundle");
    write_bundle(&root, methods, vectors);
    directory
}

/// Writes a synthetic bundle rooted at `root`.
///
/// # Panics
///
/// As [`generate_bundle`].
pub fn write_bundle(root: &Path, methods: usize, vectors: usize) {
    let write = |name: &str, contents: String| {
        let path = root.join(name);
        std::fs::write(&path, contents)
            .unwrap_or_else(|problem| panic!("{} could not be written: {problem}", path.display()));
    };

    write("profile.yaml", entry_point(methods, vectors));
    write("methods.yaml", methods_document(methods));
    write("authorization.yaml", authorization_document(methods));
    write("events.yaml", "events: []\n".to_owned());
    write("behavior.yaml", "behaviors: []\n".to_owned());
    write("invariants.yaml", "invariants: []\n".to_owned());
    write("failures.yaml", "failures: []\n".to_owned());

    // One operation directory per vector, because that is the shape the corpus loader
    // walks: a directory name is an operation, and the documents inside it are vectors.
    let directory = root.join("vectors").join("generated");
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|problem| panic!("the vector directory could not be created: {problem}"));
    for index in 0..vectors {
        let path = directory.join(format!("vector-{index}.yaml"));
        std::fs::write(&path, vector_document(index))
            .unwrap_or_else(|problem| panic!("{} could not be written: {problem}", path.display()));
    }
}

/// The entry point of a synthetic bundle.
fn entry_point(methods: usize, vectors: usize) -> String {
    format!(
        "estamora_spec_version: \"1.0\"\n\
         \n\
         profile:\n\
         \x20 id: generated\n\
         \x20 version: \"1.0\"\n\
         \x20 title: Generated Profile\n\
         \x20 status: experimental\n\
         \x20 summary: A bundle generated to measure how loading scales with its size.\n\
         \x20 description: >-\n\
         \x20   Generated by the benchmark harness. It is a valid bundle rather than a\n\
         \x20   malformed one, because the number a benchmark reports should be the cost of\n\
         \x20   the work and not of an error path.\n\
         \x20 license: Apache-2.0\n\
         \x20 specification:\n\
         \x20   name: GENERATED-0000\n\
         \x20   title: Generated Profile\n\
         \x20   version: \"1.0\"\n\
         \x20   status: draft\n\
         \x20   url: https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner\n\
         \x20   updated: \"2026-01-01\"\n\
         \x20 maintainers:\n\
         \x20   - benchmark\n\
         \x20 compatibility:\n\
         \x20   interface: partial\n\
         \x20   notes:\n\
         \x20     - Generated for benchmarking; it describes no deployed interface.\n\
         \x20 provenance:\n\
         \x20   source: composite\n\
         \x20   derived_from: generated by benches/src/lib.rs\n\
         \x20   interpretation_notes:\n\
         \x20     - No upstream text was read; this bundle exists to be loaded.\n\
         \n\
         includes:\n\
         \x20 methods: methods.yaml\n\
         \x20 authorization: authorization.yaml\n\
         \x20 events: events.yaml\n\
         \x20 behavior: behavior.yaml\n\
         \x20 invariants: invariants.yaml\n\
         \x20 failures: failures.yaml\n\
         \x20 vectors:\n\
         \x20   - generated\n\
         \x20 shared_vectors: []\n\
         \n\
         # {methods} methods, {vectors} vectors.\n"
    )
}

/// The interface of a synthetic bundle.
fn methods_document(count: usize) -> String {
    let mut text = String::from("methods:\n");
    for index in 0..count {
        // A read of one argument, so that the type expressions and the argument
        // authorization block are both parsed for every method. The cost of loading a
        // profile is dominated by the number of these, not by their depth.
        let _ = write!(
            text,
            "  - id: method-{index}\n\
             \x20   name: method_{index}\n\
             \x20   requirement: required\n\
             \x20   summary: Read a value for the generated benchmark profile.\n\
             \x20   description: >-\n\
             \x20     Generated. It takes one address and returns an integer, which is enough\n\
             \x20     structure for every part of the loader that has work to do.\n\
             \x20   args:\n\
             \x20     - name: address\n\
             \x20       type:\n\
             \x20         kind: prim\n\
             \x20         name: address\n\
             \x20       semantics: The account the value is read for.\n\
             \x20       authorization:\n\
             \x20         required: false\n\
             \x20         semantics: A read of public information, so no signature is required.\n\
             \x20   returns:\n\
             \x20     type:\n\
             \x20       kind: prim\n\
             \x20       name: i128\n\
             \x20     semantics: The value held by the address, or zero when it holds none.\n\
             \x20   mutability: readonly\n\
             \x20   invocation: read_only\n\
             \x20   authorization: []\n\
             \x20   events: []\n\
             \x20   failures: []\n\
             \x20   behaviors: []\n"
        );
    }
    text
}

/// The authorization document of a synthetic bundle.
fn authorization_document(methods: usize) -> String {
    let mut text = String::from(
        "authorization_rules:\n  - id: generated-reads-need-no-authorization\n    summary: No principal may be asked to authorize a read.\n    description: >-\n      Generated. Covers every method in the bundle so that the cross-reference from each\n      method to this rule has something to resolve against.\n    methods:\n",
    );
    for index in 0..methods {
        let _ = writeln!(text, "      - method-{index}");
    }
    text.push_str(
        "    actor:\n\
         \x20     kind: none\n\
         \x20   coverage:\n\
         \x20     mode: at_most\n\
         \x20     arguments: []\n\
         \x20   unauthorized:\n\
         \x20     kind: succeed\n\
         \x20   wrong_actor:\n\
         \x20     kind: succeed\n\
         \x20   replay_sensitive: false\n",
    );
    text
}

/// One vector of a synthetic bundle.
fn vector_document(index: usize) -> String {
    format!(
        "id: generated-vector-{index}\n\
         profile: generated\n\
         profile_version: \"1.0\"\n\
         title: A generated vector\n\
         description: >-\n\
         \x20 Generated by the benchmark harness so that the corpus loader has documents to\n\
         \x20 walk, resolve and check against the profile they belong to.\n\
         kind: positive\n\
         method: method_{index}\n\
         tags: [generated]\n\
         \n\
         fixtures:\n\
         \x20 actors:\n\
         \x20   - name: alice\n\
         \x20     kind: account\n\
         \x20     description: The account the generated read is made for.\n\
         \x20 balances:\n\
         \x20   alice: \"1000\"\n\
         \x20 allowances: []\n\
         \x20 total_supply: \"1000\"\n\
         \x20 ledger:\n\
         \x20   sequence: 1000\n\
         \x20   timestamp: \"2026-01-01T00:00:00Z\"\n\
         \x20 authorization:\n\
         \x20   alice: true\n\
         \n\
         inputs:\n\
         \x20 address:\n\
         \x20   kind: actor\n\
         \x20   ref: alice\n\
         \n\
         authorization:\n\
         \x20 actors: []\n\
         \x20 expected: not_required\n\
         \n\
         expected:\n\
         \x20 outcome: success\n\
         \x20 returns:\n\
         \x20   kind: literal\n\
         \x20   value: \"1000\"\n\
         \x20 state_assertions: []\n\
         \x20 events:\n\
         \x20   required: []\n\
         \x20   forbidden: []\n\
         \x20 invariants: []\n\
         \n\
         assertions: []\n\
         \n\
         rationale: >-\n\
         \x20 Generated. There is no situation behind it beyond the loader having something\n\
         \x20 to load.\n"
    )
}
