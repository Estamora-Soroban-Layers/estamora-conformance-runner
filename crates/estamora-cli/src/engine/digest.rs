//! Pinning the requirements a result is attributed to.
//!
//! A profile version does not identify a revision. A behavioural requirement can be
//! tightened, an invariant added, a vector rewritten, and the version string stay the
//! same — and a consumer comparing only versions would then be comparing two different
//! standards. So a report records a content digest of the bundle and of the corpus that
//! was actually executed, and a receipt commits to the same two values.
//!
//! # Why the digest is folded per file rather than taken over a buffer
//!
//! A corpus is input from outside this repository and can hold thousands of documents.
//! Reading every one into a single buffer to hash it would make the runner's memory
//! use a function of the corpus it was handed, which is exactly the shape of failure an
//! untrusted input must not be able to cause. Each document is hashed on its own and
//! only the hashes are accumulated, so the peak allocation is one document.
//!
//! # Why the path is hashed too
//!
//! Two files with the same contents in different places are different contributions to
//! a corpus: a vector moved from `transfer/` to `transfer-from/` changes which
//! requirement set it belongs to. Folding the path in makes a reorganisation visible,
//! which is what a digest of a *corpus* rather than of a bag of bytes has to do.

use std::path::{Path, PathBuf};

use estamora_certification::Digest;
use estamora_core::{Error, ErrorClass, Result};
use estamora_vectors::Vector;

/// How deep the bundle walk descends.
///
/// The layout is `profiles/<id>/<version>/` with documents and one `vectors/` tree
/// beside them, so the bound is generous and finite, which is the point: it exists so
/// that a symlink loop or a hostile tree fails with a message rather than by walking.
pub const MAX_BUNDLE_DEPTH: usize = 8;

/// The largest number of files a bundle digest will read.
pub const MAX_BUNDLE_FILES: usize = 4_096;

/// The content digest of a profile bundle.
///
/// # Errors
///
/// Returns a profile error when the bundle cannot be walked or a document cannot be
/// read. A digest that silently skipped an unreadable file would pin a revision other
/// than the one that was executed.
pub fn profile(root: &Path) -> Result<Digest> {
    let mut files = Vec::new();
    collect(root, 0, &mut files)?;
    files.sort();

    let mut folded = String::new();
    for path in &files {
        let bytes = std::fs::read(path).map_err(|problem| {
            Error::new(
                ErrorClass::ProfileError,
                format!(
                    "the profile document at {} could not be read for digesting: {problem}",
                    path.display()
                ),
            )
        })?;
        let relative = path.strip_prefix(root).unwrap_or(path);
        folded.push_str(&relative.display().to_string());
        folded.push(':');
        folded.push_str(&Digest::of_bytes(&bytes).to_string());
        folded.push('\n');
    }
    Ok(Digest::of_bytes(folded.as_bytes()))
}

/// The content digest of the vectors a run executed.
///
/// # Errors
///
/// Returns a vector error when a vector file cannot be re-read for digesting.
pub fn corpus(vectors: &[&Vector]) -> Result<Digest> {
    let mut entries: Vec<(PathBuf, &Vector)> = vectors
        .iter()
        .map(|vector| (vector.path().to_path_buf(), *vector))
        .collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0));

    let mut folded = String::new();
    for (path, _) in &entries {
        let bytes = std::fs::read(path).map_err(|problem| {
            Error::new(
                ErrorClass::VectorError,
                format!(
                    "the vector at {} could not be read for digesting: {problem}",
                    path.display()
                ),
            )
            .with_context("vector", path.display().to_string())
        })?;
        folded.push_str(&path.display().to_string());
        folded.push(':');
        folded.push_str(&Digest::of_bytes(&bytes).to_string());
        folded.push('\n');
    }
    Ok(Digest::of_bytes(folded.as_bytes()))
}

/// Collects every file under `root`, bounded in depth and count.
fn collect(root: &Path, depth: usize, into: &mut Vec<PathBuf>) -> Result<()> {
    if depth > MAX_BUNDLE_DEPTH {
        return Err(Error::new(
            ErrorClass::ProfileError,
            format!(
                "the bundle below {} is deeper than {MAX_BUNDLE_DEPTH} levels, which the layout \
                 does not reach",
                root.display()
            ),
        ));
    }
    let entries = std::fs::read_dir(root).map_err(|problem| {
        Error::new(
            ErrorClass::ProfileError,
            format!("{} could not be listed: {problem}", root.display()),
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|problem| {
            Error::new(
                ErrorClass::ProfileError,
                format!("{} could not be listed: {problem}", root.display()),
            )
        })?;
        // Taken from the directory entry rather than by following the path, so a
        // symlinked directory is neither descended into nor read. A tree that can reach
        // outside itself through a link is a tree whose digest is not the digest of the
        // bundle the profile names.
        let file_type = entry.file_type().map_err(|problem| {
            Error::new(
                ErrorClass::ProfileError,
                format!(
                    "{} could not be inspected: {problem}",
                    entry.path().display()
                ),
            )
        })?;
        let path = entry.path();
        if file_type.is_dir() {
            collect(&path, depth + 1, into)?;
        } else if file_type.is_file() {
            if into.len() >= MAX_BUNDLE_FILES {
                return Err(Error::new(
                    ErrorClass::ProfileError,
                    format!(
                        "the bundle below {} holds more than {MAX_BUNDLE_FILES} files",
                        root.display()
                    ),
                ));
            }
            into.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use std::fs;

    use super::{corpus, profile};

    #[test]
    fn a_bundle_digest_changes_when_a_document_does() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("methods.yaml"), "methods: []\n").unwrap();
        let first = profile(directory.path()).unwrap();
        fs::write(
            directory.path().join("methods.yaml"),
            "methods:\n  - id: x\n",
        )
        .unwrap();
        let second = profile(directory.path()).unwrap();
        assert_ne!(
            first, second,
            "a changed requirement must change the digest, or a result can be re-attributed to it"
        );
    }

    #[test]
    fn a_bundle_digest_is_stable_across_two_readings() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("a.yaml"), "a: 1\n").unwrap();
        fs::write(directory.path().join("b.yaml"), "b: 2\n").unwrap();
        assert_eq!(
            profile(directory.path()).unwrap(),
            profile(directory.path()).unwrap()
        );
    }

    #[test]
    fn moving_a_document_changes_the_digest_of_the_bundle() {
        // The path is part of the contribution: a vector reorganised into a different
        // operation directory belongs to a different requirement set.
        let first = tempfile::tempdir().unwrap();
        fs::create_dir(first.path().join("transfer")).unwrap();
        fs::write(first.path().join("transfer/v.yaml"), "id: v\n").unwrap();

        let second = tempfile::tempdir().unwrap();
        fs::create_dir(second.path().join("transfer-from")).unwrap();
        fs::write(second.path().join("transfer-from/v.yaml"), "id: v\n").unwrap();

        assert_ne!(
            profile(first.path()).unwrap(),
            profile(second.path()).unwrap()
        );
    }

    /// The gate is on the whole test rather than on the `symlink` call, deliberately.
    ///
    /// With the call gated inside the body, the test still *ran* on Windows and still
    /// passed: it asserted that two readings of an unchanged bundle agree, which the test
    /// above already asserts and which says nothing about links. A test that passes
    /// without exercising its subject is worse than an absent one, because the count of
    /// passing tests is what a reader checks.
    ///
    /// The behaviour itself is Unix-specific: creating a directory symlink on Windows
    /// needs a privilege the test runner may not hold, and `std::os::windows` offers a
    /// different mechanism with different semantics. Nothing is asserted there rather
    /// than something that would pass for the wrong reason.
    #[cfg(unix)]
    #[test]
    fn a_remote_symlink_is_not_followed() {
        // A tree that can reach outside itself through a link is not the tree the
        // profile names, and a digest that followed one would pin the wrong revision.
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.yaml"), "secret: true\n").unwrap();
        let bundle = tempfile::tempdir().unwrap();
        fs::write(bundle.path().join("methods.yaml"), "methods: []\n").unwrap();
        std::os::unix::fs::symlink(outside.path(), bundle.path().join("link")).unwrap();

        let digest = profile(bundle.path()).unwrap();
        fs::write(outside.path().join("secret.yaml"), "secret: changed\n").unwrap();
        assert_eq!(
            digest,
            profile(bundle.path()).unwrap(),
            "a symlinked directory must not contribute to the digest"
        );
    }

    #[test]
    fn an_empty_corpus_has_a_digest_of_its_own() {
        // Not an error: a digest of nothing is a well-defined value, and the run's
        // verdict for an empty corpus is decided by the tally rather than here.
        assert_eq!(
            corpus(&[]).unwrap(),
            estamora_certification::Digest::of_bytes(b"")
        );
    }
}
