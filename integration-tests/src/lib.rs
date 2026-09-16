//! The harness the end-to-end tests share.
//!
//! Three kinds of question are answered here. What a run against an in-repository
//! fixture concluded, which the dimension tests use. What a run does with a document
//! that is wrong, which the malformed-input tests use and which needs a temporary copy
//! of the bundle to answer. And what a *stored* report re-renders to, which the report
//! and receipt tests use.
//!
//! Everything here is a way of asking the runner a question and reading the answer
//! through the same API a user drives. Nothing reaches into a crate's internals, so a
//! test written against this harness fails when the *product* breaks rather than when
//! an internal signature changes — which is the only kind of failure an end-to-end
//! suite is worth having for.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a harness used only by tests may panic; a failing test is the signal"
)]

use std::path::{Path, PathBuf};

pub use tempfile::TempDir;

use estamora_cli::RunOutcome;
use estamora_cli::engine::Target;
use estamora_core::{AssertionStatus, ConformanceStatus};
use estamora_report::Report;

/// The profile the fixtures are measured against, as `--profile` names it.
///
/// It is the runner's own bundle rather than SEP-41, so that these tests run on a
/// checkout with no sibling specification repository. It declares the same `transfer`
/// and `balance` requirements for the subset it covers, and the tests that need the
/// real standard are the cross-repository ones.
pub const FIXTURE_PROFILE: &str = "conformance-token@1.0";

/// Where the malformed documents live, each laid out at the path it occupies in a bundle.
#[must_use]
pub fn malformed_inputs_root() -> PathBuf {
    fixture_spec_root().join("malformed-inputs")
}

/// The repository root, derived from this crate's own directory.
///
/// `join("..")` rather than a canonicalised path, because the path arithmetic is not
/// the point: every use of it is a filesystem read that fails loudly and specifically
/// if the layout has changed.
#[must_use]
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The tree the profile bundle lives in, passed to the runner as its `--spec`.
///
/// `fixtures/` is laid out the way a specification checkout is — `profiles/<id>/<version>/`
/// — so the runner resolves `conformance-token@1.0` against it exactly as it resolves
/// `sep-41@1.0` against the real repository. That is what makes these tests exercise the
/// same resolution path a user's run takes rather than a private one.
#[must_use]
pub fn fixture_spec_root() -> PathBuf {
    repo_root().join("fixtures")
}

/// The directory of the fixture profile bundle.
#[must_use]
pub fn fixture_profile_root() -> PathBuf {
    fixture_spec_root().join("profiles/conformance-token/1.0")
}

/// The compiled fixture contract, the one the `.wasm` target is measured against.
///
/// Committed rather than built by the test, so that the artifact target is covered by
/// the default suite: a test that first had to compile a contract for WebAssembly would
/// be slow, would need a target many machines lack, and would not run at all on a
/// checkout that is only being read. `scripts/build-fixture-wasm.sh` produces it and CI
/// rebuilds it and fails if the committed copy differs.
///
/// # Panics
///
/// Aborts when the artifact is not there. That is a defect in this repository — the file
/// is tracked — rather than a result about any contract, and a test that quietly skipped
/// would report the artifact target as covered when nothing had measured it.
#[must_use]
pub fn fixture_wasm_path() -> PathBuf {
    let path = repo_root().join("fixtures/wasm/measurable-token.wasm");
    assert!(
        path.is_file(),
        "the compiled fixture artifact is missing at {}; run scripts/build-fixture-wasm.sh",
        path.display()
    );
    path
}

/// Runs the fixture profile against the compiled fixture artifact.
///
/// # Panics
///
/// Aborts when the artifact cannot be named as a target or the run cannot be attempted.
#[must_use]
pub fn run_artifact() -> RunOutcome {
    let contract = fixture_wasm_path();
    let contract = contract
        .to_str()
        .unwrap_or_else(|| panic!("{} is not a UTF-8 path", contract.display()));
    run_target(contract, None).unwrap_or_else(|problem| {
        panic!("the compiled fixture artifact could not be measured: {problem}")
    })
}

/// Runs the fixture profile against one of the in-repository fixtures.
///
/// # Panics
///
/// Aborts when the target cannot be named or the run cannot be attempted. Both are
/// defects in the test rather than results about a contract.
#[must_use]
pub fn run_fixture(defect: &str) -> RunOutcome {
    run_fixture_with_tags(defect, &[])
}

/// Runs the fixture profile against a fixture, selecting vectors by tag.
///
/// # Panics
///
/// As [`run_fixture`].
#[must_use]
pub fn run_fixture_with_tags(defect: &str, tags: &[&str]) -> RunOutcome {
    let config = fixture_config(defect, tags);
    estamora_cli::run(&config)
        .unwrap_or_else(|problem| panic!("the fixture `{defect}` could not be measured: {problem}"))
}

/// The configuration for a fixture run, without executing it.
///
/// Exposed so that a test about a *configuration* — a tag filter that selects nothing,
/// for instance — can assert on the arguments rather than on a verdict.
///
/// # Panics
///
/// Aborts when `defect` does not name one of the fixtures, which is a defect in the test
/// rather than a result about a contract.
#[must_use]
pub fn fixture_config(defect: &str, tags: &[&str]) -> estamora_cli::RunConfig {
    let target = Target::parse(&format!("fixture:{defect}"), None)
        .unwrap_or_else(|problem| panic!("`fixture:{defect}` is not a target: {problem}"));
    let mut config =
        estamora_cli::RunConfig::new(fixture_profile_root(), fixture_spec_root(), target);
    config.tags = tags.iter().map(|tag| (*tag).to_owned()).collect();
    config
}

/// Reports that a test did nothing, because its environment was not configured.
///
/// A test that needs a network and has none is neither a pass nor a failure, and the
/// difference matters: reporting it as a pass is how a suite that checks nothing comes
/// to look green. The reason is written to standard error, where a test runner shows it,
/// rather than asserted, because there is nothing to assert.
///
/// The suppression is the reason `allow_attributes_without_reason` is denied
/// workspace-wide. `print_stderr` exists so that the runner cannot write alongside its
/// own structured output, and this is not the runner: it is a test reporting why it
/// stood down.
#[allow(
    clippy::print_stderr,
    reason = "a test that stands down has to say so in the test runner's output, and it is \
              not the CLI's structured output that the lint protects"
)]
pub fn report_skip(what: &str) {
    eprintln!("{what}");
}

/// Runs the fixture profile against a target named as `--contract` would name it.
///
/// `network` is required for a deployed contract and ignored otherwise. Returned rather
/// than unwrapped because the tests that use it are about how a target that cannot be
/// *reached* is reported, which is an outcome rather than a defect in the test.
///
/// # Errors
///
/// Returns a usage error for a contract that is neither a fixture nor a well-formed
/// identifier, a network error when the endpoint cannot be reached, and a contract
/// resolution error when the contract cannot be read. None of those is a verdict.
pub fn run_target(contract: &str, network: Option<&str>) -> estamora_core::Result<RunOutcome> {
    let target = Target::parse(contract, network)?;
    estamora_cli::run(&estamora_cli::RunConfig::new(
        fixture_profile_root(),
        fixture_spec_root(),
        target,
    ))
}

/// Runs a profile bundle at an arbitrary path against a fixture.
///
/// Used by the tests about malformed input, where the point is that the run is refused
/// with a specific classification rather than that a verdict is reached.
///
/// # Errors
///
/// Returns whatever the run returned: a profile error for an unusable bundle, a vector
/// error for an unusable corpus, and an execution error when a vector's declared world
/// cannot be put into the contract.
pub fn run_bundle(path: &Path, defect: &str) -> estamora_core::Result<RunOutcome> {
    let target = Target::parse(&format!("fixture:{defect}"), None)?;
    estamora_cli::run(&estamora_cli::RunConfig::new(
        path,
        fixture_spec_root(),
        target,
    ))
}

/// The stored report produced by a known run against the conforming fixture.
///
/// Its timestamp is pinned, so the document is stable and a test can assert that reading
/// and re-rendering it reproduces it byte for byte. A fixture whose own content moved
/// between runs could not pin anything.
#[must_use]
pub fn stored_report_path() -> PathBuf {
    repo_root().join("fixtures/expected-reports/conformant-transfer.json")
}

/// The report of the measurement committed beside the example that produced it.
///
/// This one was made over RPC against a contract deployed to testnet, so it is the only
/// report in the repository that a network produced. It is read by a test for a second
/// reason beyond re-rendering: it is a document written by an earlier release, and a
/// change to the report model that made it unreadable would break every receipt that
/// commits to a report the runner no longer produces.
#[must_use]
pub fn evidence_report_path() -> PathBuf {
    repo_root().join("examples/testnet-contract/report.json")
}

/// A temporary copy of the fixture bundle with one of the malformed documents over it.
///
/// The returned guard owns the directory, so the caller does not have to remove it and a
/// failed assertion cannot leave it behind.
///
/// # Panics
///
/// Aborts when the bundle or the named fixture cannot be copied, which is a defect in the
/// fixture set rather than a result about a contract.
#[must_use]
pub fn bundle_with(case: &str) -> TempDir {
    let directory = TempDir::new()
        .unwrap_or_else(|problem| panic!("a temporary directory could not be created: {problem}"));
    let root = directory.path().join("bundle");
    copy_tree(&fixture_profile_root(), &root);
    copy_tree(&malformed_inputs_root().join(case), &root);
    directory
}

/// The bundle directory inside the guard [`bundle_with`] returned.
#[must_use]
pub fn bundle_root(directory: &TempDir) -> PathBuf {
    directory.path().join("bundle")
}

/// A temporary copy of the whole fixture specification tree.
///
/// Returns the guard, the `--spec` root inside the copy and the profile root inside the
/// copy, in that order.
///
/// A digest of a corpus is supposed to be a function of the corpus, and the only way to
/// ask whether it is one is to put the same corpus somewhere else and hash it again.
/// [`bundle_with`] cannot answer that: it copies one bundle and lays a malformed document
/// over it, which is a different question. This copies the tree a `--spec` points at, so a
/// run against the copy exercises the same resolution a run against the repository does
/// while every path it reads has moved.
///
/// # Panics
///
/// Aborts when the fixture tree cannot be copied, which is a defect in the fixture set
/// rather than a result about a contract.
#[must_use]
pub fn spec_tree_copy() -> (TempDir, PathBuf, PathBuf) {
    let directory = TempDir::new()
        .unwrap_or_else(|problem| panic!("a temporary directory could not be created: {problem}"));
    let root = directory.path().join("spec");
    copy_tree(&fixture_spec_root(), &root);
    let profile = root.join("profiles/conformance-token/1.0");
    (directory, root, profile)
}

/// Copies a directory tree, creating the destination and its parents.
///
/// Iterative rather than recursive, and it does not follow a symlink out of the tree it
/// was given: a fixture set that could be made to walk outside its own directory would be
/// a way to make a test read something it did not name.
fn copy_tree(from: &Path, to: &Path) {
    let mut pending = vec![(from.to_path_buf(), to.to_path_buf())];
    while let Some((source, destination)) = pending.pop() {
        if source.is_dir() {
            std::fs::create_dir_all(&destination).unwrap_or_else(|problem| {
                panic!("{} could not be created: {problem}", destination.display())
            });
            let entries = std::fs::read_dir(&source).unwrap_or_else(|problem| {
                panic!("{} could not be read: {problem}", source.display())
            });
            for entry in entries {
                let entry = entry.unwrap_or_else(|problem| {
                    panic!("{} could not be listed: {problem}", source.display())
                });
                pending.push((entry.path(), destination.join(entry.file_name())));
            }
            continue;
        }
        std::fs::copy(&source, &destination).unwrap_or_else(|problem| {
            panic!(
                "{} could not be copied to {}: {problem}",
                source.display(),
                destination.display()
            )
        });
    }
}

/// The run's verdict.
#[must_use]
pub fn status(outcome: &RunOutcome) -> ConformanceStatus {
    outcome.report.status
}

/// The normative report of a run.
#[must_use]
pub fn report(outcome: &RunOutcome) -> &Report {
    &outcome.report
}

/// Every check that did not hold, across the whole run, as `id (category)`.
#[must_use]
pub fn failures(outcome: &RunOutcome) -> Vec<String> {
    checks_matching(outcome, AssertionStatus::Failed)
}

/// Every check that held, across the whole run.
#[must_use]
pub fn passes(outcome: &RunOutcome) -> Vec<String> {
    checks_matching(outcome, AssertionStatus::Passed)
}

/// The identifiers of the checks that ended in `wanted`, in the order they were reported.
#[must_use]
pub fn checks_matching(outcome: &RunOutcome, wanted: AssertionStatus) -> Vec<String> {
    outcome
        .outcomes
        .iter()
        .flat_map(|vector| vector.assertions.iter())
        .filter(|assertion| assertion.status == wanted)
        // The identifier is the one the schema accepts, which is normalised and may be
        // hashed. The `detail` field carries the evaluator's own name for the check, so
        // that is what a reader searches: both are returned, joined, so that a test can
        // match on either without the harness deciding which one a reader wants.
        .map(|assertion| match &assertion.detail {
            Some(detail) => format!("{} {}", assertion.id, detail),
            None => assertion.id.clone(),
        })
        .collect()
}

/// Asserts that some check whose identifier or detail mentions `needle` did not hold.
///
/// # Panics
///
/// Aborts when no such check failed, listing every failure so that the reason is
/// visible from the test output rather than by re-running with a debugger.
pub fn assert_failed(outcome: &RunOutcome, needle: &str) {
    let failed = failures(outcome);
    assert!(
        failed.iter().any(|check| check.contains(needle)),
        "expected a failed check mentioning `{needle}`; the run failed: {failed:#?}"
    );
}

/// Asserts that some check whose identifier or detail mentions `needle` held.
///
/// # Panics
///
/// Aborts when no such check passed.
pub fn assert_passed(outcome: &RunOutcome, needle: &str) {
    let passed = passes(outcome);
    assert!(
        passed.iter().any(|check| check.contains(needle)),
        "expected a passing check mentioning `{needle}`; the run passed: {passed:#?}"
    );
}

/// Asserts that the run reported no failure at all.
///
/// # Panics
///
/// Aborts listing the failures.
pub fn assert_clean(outcome: &RunOutcome) {
    let failed = failures(outcome);
    assert!(
        failed.is_empty(),
        "expected no failed check; the run failed: {failed:#?}"
    );
}

/// The report rendered as the normative JSON document.
///
/// # Panics
///
/// Aborts when the document cannot be produced, which would be a defect in the writer.
#[must_use]
pub fn json(outcome: &RunOutcome) -> String {
    estamora_report::render_json(report(outcome), false)
        .unwrap_or_else(|problem| panic!("the report could not be rendered: {problem}"))
}

/// The names of the vectors a run executed, in execution order.
#[must_use]
pub fn vector_ids(outcome: &RunOutcome) -> Vec<String> {
    outcome
        .outcomes
        .iter()
        .map(|vector| vector.vector_id.clone())
        .collect()
}
