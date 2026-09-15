//! What a run is configured with.
//!
//! A conformance result is only interpretable against the configuration that
//! produced it, so this module is where the configuration is assembled and also
//! where the part of it that could change a verdict is rendered for the report. Two
//! runs of the same vectors against the same contract that disagree because one had
//! a different tag filter or a different specification checkout must be
//! distinguishable from a report alone, and the only way that is true is if the
//! configuration is in the report.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use estamora_report::RunnerIdentity;
use serde_json::Value;

use crate::engine::Target;

/// The environment variable naming a checkout of the specification repository.
///
/// The runner consumes profiles and vectors from `estamora-conformance-spec`, so it
/// has to be told where that checkout is. An environment variable is the documented
/// mechanism and the default is the sibling directory, because the two repositories
/// are developed beside each other and CI checks both out into one workspace.
pub const DEFAULT_SPEC_ROOT_ENV: &str = "ESTAMORA_SPEC_REPO";

/// The name the runner reports itself under.
pub const RUNNER_NAME: &str = "estamora";

/// The default location of the specification checkout.
///
/// `estamora-conformance-spec`, as a sibling of this repository. Used only when
/// [`DEFAULT_SPEC_ROOT_ENV`] is unset, and only when the runner is being driven from
/// a checkout of the runner: a binary installed from a release archive is not a
/// sibling of anything, so this is the case an installed runner misses.
pub const DEFAULT_SPEC_RELATIVE: &str = "../estamora-conformance-spec";

/// Where the specification repository is published.
///
/// Named here because the error below is the first thing an installed runner prints,
/// and a message that says a directory is missing without saying where to get it
/// leaves the reader to search for the repository that defines the thing they are
/// trying to measure.
pub const SPEC_REPOSITORY_URL: &str =
    "https://github.com/Estamora-Soroban-Layers/estamora-conformance-spec";

/// The runner's own version.
///
/// Independent of the profile version and of the specification format version, and
/// recorded in every report, because two runners may differ in behaviour and a report
/// that does not name its producer cannot be re-verified.
#[must_use]
pub fn runner_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The runner's identity, as a report records it.
#[must_use]
pub fn runner_identity() -> RunnerIdentity {
    RunnerIdentity {
        name: RUNNER_NAME.to_owned(),
        version: runner_version().to_owned(),
    }
}

/// The specification checkout to resolve, honouring [`DEFAULT_SPEC_ROOT_ENV`].
///
/// # Errors
///
/// Returns a profile error naming both the variable and the directory it pointed at
/// when the path is not a directory. Refusing rather than falling back is deliberate:
/// silently measuring a different checkout than the one the caller named is how a
/// result gets attributed to requirements that were never read.
pub fn spec_root() -> estamora_core::Result<PathBuf> {
    spec_root_from(
        std::env::var_os(DEFAULT_SPEC_ROOT_ENV).as_deref(),
        Path::new(DEFAULT_SPEC_RELATIVE),
    )
}

/// The decision [`spec_root`] makes, with the environment and the default passed in.
///
/// Split out so the message can be asserted on. Reading the variable inside the
/// function under test would make the test mutate process-wide state, which is both
/// racy against every other test in this binary and, in CI, already set — so the one
/// message an installed runner prints first would be the one message no test could
/// reach.
fn spec_root_from(
    configured: Option<&std::ffi::OsStr>,
    default: &Path,
) -> estamora_core::Result<PathBuf> {
    let candidate = match &configured {
        Some(value) => PathBuf::from(value),
        None => default.to_path_buf(),
    };
    if candidate.is_dir() {
        return Ok(candidate);
    }
    // Both messages end with the same remedy, because the reader's next action is the
    // same in both cases: put a checkout somewhere and name it. A release archive is
    // installed on its own, so the sibling default cannot be satisfied by having
    // installed the runner -- which is exactly when this message is reached.
    let message = if configured.is_some() {
        format!(
            "{DEFAULT_SPEC_ROOT_ENV} names `{}`, which is not a directory. Clone the \
             specification and point at it: git clone {SPEC_REPOSITORY_URL}",
            candidate.display()
        )
    } else {
        format!(
            "no specification checkout was found. The default is `{}`, which is not there, \
             and it can only be there when running from a checkout of this repository. \
             Point at one with --spec or {DEFAULT_SPEC_ROOT_ENV}, or clone it now: \
             git clone {SPEC_REPOSITORY_URL}",
            default.display()
        )
    };
    Err(
        estamora_core::Error::new(estamora_core::ErrorClass::ProfileError, message)
            .with_context("path", candidate.display().to_string()),
    )
}

/// Everything one run needs.
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// The directory of the profile bundle to execute.
    pub profile_root: PathBuf,
    /// The specification checkout the bundle's shared vectors come from.
    pub spec_root: PathBuf,
    /// What is being measured.
    pub target: Target,
    /// The vector tags to select. Empty means the whole corpus.
    pub tags: Vec<String>,
    /// When the run started, as an RFC 3339 date-time.
    pub generated_at: String,
    /// The hex-encoded seed of the key that signs a receipt, when one was asked for.
    ///
    /// A signing key is never recorded anywhere: it is not part of the configuration
    /// map and does not reach a report.
    pub signing_key: Option<String>,
    /// The path a receipt is written to, when one was asked for.
    pub receipt_path: Option<PathBuf>,
}

impl RunConfig {
    /// A configuration for `target`, measuring the bundle at `profile_root`.
    #[must_use]
    pub fn new(
        profile_root: impl Into<PathBuf>,
        spec_root: impl Into<PathBuf>,
        target: Target,
    ) -> Self {
        Self {
            profile_root: profile_root.into(),
            spec_root: spec_root.into(),
            target,
            tags: Vec::new(),
            generated_at: now(),
            signing_key: None,
            receipt_path: None,
        }
    }

    /// Selects only the vectors carrying every one of `tags`.
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Signs a receipt with the key whose 32-byte seed is `seed`, hex-encoded.
    #[must_use]
    pub fn with_signing_key(mut self, seed: impl Into<String>) -> Self {
        self.signing_key = Some(seed.into());
        self
    }

    /// Writes the receipt to `path`.
    #[must_use]
    pub fn with_receipt_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.receipt_path = Some(path.into());
        self
    }

    /// The configuration as the report records it.
    ///
    /// Only inputs that could change a verdict are recorded, and the signing key is
    /// deliberately not among them: a report is a published document, and a run that
    /// put a private key in one would be a run that leaked it.
    #[must_use]
    pub fn as_recorded(&self) -> BTreeMap<String, Value> {
        let mut recorded = BTreeMap::new();
        recorded.insert(
            "network".to_owned(),
            Value::String(self.target.network().to_owned()),
        );
        recorded.insert("contract".to_owned(), Value::String(self.target.describe()));
        recorded.insert(
            "profile_root".to_owned(),
            Value::String(self.profile_root.display().to_string()),
        );
        recorded.insert(
            "vectors_root".to_owned(),
            Value::String(self.spec_root.join("vectors").display().to_string()),
        );
        recorded.insert(
            "tags".to_owned(),
            Value::Array(
                self.tags
                    .iter()
                    .map(|tag| Value::String(tag.clone()))
                    .collect(),
            ),
        );
        recorded.insert(
            "seeding".to_owned(),
            Value::String(self.target.seeding_description().to_owned()),
        );
        recorded
    }

    /// Whether the profile bundle directory exists.
    #[must_use]
    pub fn profile_root_exists(&self) -> bool {
        self.profile_root.is_dir()
    }
}

/// The current instant, as an RFC 3339 date-time.
///
/// Formatted here rather than in the report so that every timestamp the runner
/// produces has one spelling. A report whose timestamp could not be parsed by another
/// implementation would make two runs impossible to order.
#[must_use]
pub fn now() -> String {
    time::OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

/// Resolves a `--profile` value to the directory of a bundle.
///
/// Three spellings are accepted, and all three are resolved to a directory before
/// anything is loaded:
///
/// - an existing directory holding a `profile.yaml` — the explicit form, for a bundle
///   that is not in a specification checkout at all;
/// - `id@version`, which is how a profile is named in a command line, a report and a
///   receipt, and which resolves to `profiles/<id>/<version>/`;
/// - a path relative to the checkout's `profiles/` directory, which is how a worked
///   example is named.
///
/// # Errors
///
/// Returns a profile error naming every location that was tried when the value
/// resolves to nothing. Naming the alternatives rather than only the failure is the
/// difference between a fixable command line and a guess.
pub fn resolve_profile(spec_root: &Path, text: &str) -> estamora_core::Result<PathBuf> {
    if looks_like_a_bundle(text) {
        return Ok(PathBuf::from(text));
    }

    let profiles = spec_root.join("profiles");
    let mut tried = Vec::new();

    if let Some((id, version)) = text.split_once('@') {
        let candidate = profiles.join(id).join(version);
        if looks_like_a_bundle(&candidate) {
            return Ok(candidate);
        }
        tried.push(candidate);
    } else {
        let candidate = profiles.join(text);
        if looks_like_a_bundle(&candidate) {
            return Ok(candidate);
        }
        tried.push(candidate);
    }

    Err(estamora_core::Error::new(
        estamora_core::ErrorClass::ProfileError,
        format!(
            "{text:?} does not name a profile bundle: it is not a directory holding a \
             {} and nothing was found at {}",
            estamora_profile::BUNDLE_ENTRY_POINT,
            tried
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<String>>()
                .join(", ")
        ),
    )
    .with_context("profiles", profiles.display().to_string()))
}

/// Whether `path` looks like a profile bundle rather than a profile reference.
///
/// A bundle is a directory, so this is a question about the filesystem and not about
/// the text. It exists so that `--profile` can accept either a path or an `id@version`
/// reference without guessing: a reference is never a path that exists.
#[must_use]
pub fn looks_like_a_bundle(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    path.is_dir() && path.join(estamora_profile::BUNDLE_ENTRY_POINT).is_file()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{RunConfig, looks_like_a_bundle, runner_identity, spec_root_from};
    use crate::engine::Target;

    #[test]
    fn a_missing_specification_checkout_says_how_to_get_one() {
        // This is the first thing an installed runner prints: a binary from a release
        // archive is a sibling of nothing, so the default cannot be satisfied by
        // having installed it. A message that only said a directory was missing would
        // leave the reader hunting for the repository that defines the requirements
        // they are trying to measure, so the remedy is part of the contract.
        let absent = tempfile::tempdir().unwrap();
        let default = absent.path().join("estamora-conformance-spec");

        let problem = spec_root_from(None, &default).unwrap_err();
        assert_eq!(problem.class(), estamora_core::ErrorClass::ProfileError);
        assert!(
            problem.message().contains(&default.display().to_string()),
            "the failure must name the default it tried: {}",
            problem.message()
        );
        assert!(
            problem.message().contains(super::SPEC_REPOSITORY_URL),
            "the failure must say where to clone it from: {}",
            problem.message()
        );
        assert!(
            problem.message().contains("ESTAMORA_SPEC_REPO")
                && problem.message().contains("--spec"),
            "the failure must name both ways to point at a checkout: {}",
            problem.message()
        );
    }

    #[test]
    fn a_configured_checkout_that_is_not_there_is_named_with_the_clone_command() {
        let absent = tempfile::tempdir().unwrap();
        let named = absent.path().join("somewhere-else");

        let problem = spec_root_from(Some(named.as_os_str()), absent.path()).unwrap_err();
        assert_eq!(problem.class(), estamora_core::ErrorClass::ProfileError);
        assert!(
            problem.message().contains(&named.display().to_string()),
            "the failure must name what the variable held: {}",
            problem.message()
        );
        assert!(
            problem.message().contains(super::SPEC_REPOSITORY_URL),
            "the failure must say where to clone it from: {}",
            problem.message()
        );
    }

    #[test]
    fn a_checkout_that_exists_is_returned_unchanged() {
        let present = tempfile::tempdir().unwrap();
        let configured = present.path().join("spec");
        std::fs::create_dir_all(&configured).unwrap();

        // Both routes are asserted, because the default is what a checkout of this
        // repository relies on and the variable is what CI and every consumer sets.
        assert_eq!(
            spec_root_from(Some(configured.as_os_str()), present.path()).unwrap(),
            configured
        );
        assert_eq!(
            spec_root_from(None, present.path()).unwrap(),
            present.path().to_path_buf()
        );
    }

    #[test]
    fn the_recorded_configuration_names_everything_that_could_change_a_verdict() {
        let config = RunConfig::new(
            "/profiles/sep-41/1.0",
            "/spec",
            Target::parse("fixture:none", None).unwrap(),
        )
        .with_tags(vec!["authorization".to_owned()]);
        let recorded = config.as_recorded();
        for key in [
            "network",
            "contract",
            "profile_root",
            "vectors_root",
            "tags",
            "seeding",
        ] {
            assert!(recorded.contains_key(key), "{key} must be recorded");
        }
        assert_eq!(recorded["tags"], serde_json::json!(["authorization"]));
    }

    #[test]
    fn a_signing_key_never_reaches_the_configuration_a_report_carries() {
        // A report is published. A run that recorded its signing seed would be a run
        // that leaked it, so the key is checked to be absent rather than assumed so.
        let config = RunConfig::new("/p", "/s", Target::parse("fixture:none", None).unwrap())
            .with_signing_key("aa".repeat(32));
        let rendered = serde_json::to_string(&config.as_recorded()).unwrap();
        assert!(config.signing_key.is_some());
        assert!(
            !rendered.contains(&"aa".repeat(32)),
            "the signing key must not be recorded"
        );
    }

    #[test]
    fn a_profile_is_resolved_from_a_reference_a_relative_path_or_a_directory() {
        let checkout = tempfile::tempdir().unwrap();
        for relative in ["sep-41/1.0", "examples/minimal-token"] {
            let root = checkout.path().join("profiles").join(relative);
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join(estamora_profile::BUNDLE_ENTRY_POINT), "").unwrap();
        }

        let from_reference = super::resolve_profile(checkout.path(), "sep-41@1.0").unwrap();
        assert_eq!(from_reference, checkout.path().join("profiles/sep-41/1.0"));

        let from_relative = super::resolve_profile(checkout.path(), "sep-41/1.0").unwrap();
        assert_eq!(from_relative, from_reference);

        let example = super::resolve_profile(checkout.path(), "examples/minimal-token").unwrap();
        assert!(example.ends_with("profiles/examples/minimal-token"));
    }

    #[test]
    fn an_unresolvable_profile_says_where_it_looked() {
        let checkout = tempfile::tempdir().unwrap();
        let problem = super::resolve_profile(checkout.path(), "nope@9.9").unwrap_err();
        assert_eq!(problem.class(), estamora_core::ErrorClass::ProfileError);

        // The message renders the path in the platform's own form, so the assertion
        // builds the same path the resolver would rather than spelling it with the
        // separator of one platform. Both strings come from `display()` on paths built
        // by the same `join` calls, so they agree everywhere by construction instead of
        // by the good fortune of the machine the test runs on.
        //
        // Naming the whole path is also the stronger claim, and the one the test's name
        // makes: the message must say where it looked, not merely contain a plausible
        // suffix.
        let tried = checkout.path().join("profiles").join("nope").join("9.9");
        assert!(
            problem.message().contains(&tried.display().to_string()),
            "the failure must name the location tried: {}",
            problem.message()
        );
    }

    #[test]
    fn a_reference_is_not_mistaken_for_a_bundle() {
        assert!(!looks_like_a_bundle("sep-41@1.0"));
        assert!(!looks_like_a_bundle("/nonexistent/path/sep-41/1.0"));
    }

    #[test]
    fn the_runner_identifies_itself_by_a_version_that_is_not_a_profile_version() {
        let identity = runner_identity();
        assert_eq!(identity.name, "estamora");
        assert!(!identity.version.is_empty());
    }
}
