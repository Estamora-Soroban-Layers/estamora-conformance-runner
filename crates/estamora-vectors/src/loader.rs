//! Loading a vector corpus.
//!
//! A corpus is not a directory listing. It is the set of vectors a profile
//! *declares*: the operation directories the bundle owns under `vectors/`, plus
//! the shared families under the specification repository's `vectors/` that the
//! bundle declares it consumes. Anything else in either tree belongs to another
//! profile and is deliberately not swept up, because a profile that silently
//! inherited a sibling's vectors would be measured against requirements it never
//! claimed.
//!
//! # Determinism
//!
//! Files are sorted before they are read, so two runs over the same tree produce
//! the same corpus in the same order. A run whose *contents* depend on the order a
//! filesystem happened to return entries is a run whose report cannot be compared
//! with another.
//!
//! # Bounds
//!
//! A corpus is input from outside this repository. The walk is bounded in depth and
//! in how many vectors it will accept, symlinked directories are not followed, and
//! each document is bounded in size, so that a hostile or merely broken tree fails
//! with a message rather than by exhausting the run.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use estamora_core::{Diagnostic, Diagnostics, Error, ErrorClass, Result, into_result};
use estamora_profile::{MAX_DOCUMENT_BYTES, ProfileBundle};

use crate::model::VectorDocument;
use crate::validate;

/// The largest number of vectors a corpus will accept.
pub const MAX_VECTORS: usize = 10_000;

/// How deep the walk descends below a declared directory.
///
/// The layout is `<family>/<operation>/<vector>.yaml`, so three levels is the
/// deepest a real tree reaches. The bound is generous and finite, which is the
/// point: it exists to stop a symlink loop or a hostile tree from being walked
/// without end.
pub const MAX_WALK_DEPTH: usize = 8;

/// One vector, with the path it was read from.
#[derive(Debug, Clone)]
pub struct Vector {
    path: PathBuf,
    document: VectorDocument,
}

impl Vector {
    /// The file it was read from.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The parsed document.
    #[must_use]
    pub const fn document(&self) -> &VectorDocument {
        &self.document
    }

    /// The vector's globally unique identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.document.id
    }

    /// Whether the vector belongs to no profile in particular.
    #[must_use]
    pub fn is_profile_independent(&self) -> bool {
        self.document.profile == "*"
    }
}

/// The vectors a profile declares, loaded and checked.
#[derive(Debug, Clone)]
pub struct VectorCorpus {
    vectors: Vec<Vector>,
    skipped: Vec<(String, String)>,
    diagnostics: Diagnostics,
}

impl VectorCorpus {
    /// Loads the corpus `bundle` declares.
    ///
    /// `shared_root` is the specification repository's `vectors/` directory. It is
    /// consulted only for the families the bundle declares, so a profile that
    /// consumes no shared vectors does not depend on it existing.
    ///
    /// # Errors
    ///
    /// Returns a vector error if a declared directory does not exist or holds no
    /// vectors, if the walk exceeds [`MAX_WALK_DEPTH`] or [`MAX_VECTORS`], if a
    /// document is unreadable, oversized or malformed, if two vectors share an
    /// identifier, or if any vector refers to something the profile does not
    /// declare.
    pub fn load(bundle: &ProfileBundle, shared_root: impl AsRef<Path>) -> Result<Self> {
        let shared_root = shared_root.as_ref();
        let root = bundle.root();

        let mut paths = Vec::new();
        for directory in bundle.vector_directories() {
            let declared = root.join("vectors").join(directory);
            paths.append(&mut declared_tree(
                &declared,
                "the bundle's own vectors",
                directory,
            )?);
        }
        for family in bundle.shared_vector_families() {
            let declared = shared_root.join(family);
            paths.append(&mut declared_tree(
                &declared,
                "the shared vector library",
                family,
            )?);
        }

        // A file declared twice — which a profile could arrange by naming one
        // directory under both `vectors` and `shared_vectors` — is read once, so
        // that its id is not reported as a duplicate it is not.
        paths.sort();
        paths.dedup();

        let mut diagnostics = Diagnostics::new();
        let mut vectors = Vec::new();
        let mut skipped = Vec::new();
        let mut seen_ids = BTreeSet::new();

        for path in paths {
            let document: VectorDocument = read_yaml(&path)?;

            // A profile-independent vector describes a scenario every profile of a
            // family must satisfy, but it still names a method. A profile that does
            // not implement that method is not a profile the vector applies to, so
            // it is excluded — visibly, with a warning, rather than silently, and
            // rather than being reported as a defect in the profile.
            if document.profile == "*" && bundle.documents().method(&document.method).is_none() {
                diagnostics.push(Diagnostic::warning(
                    ErrorClass::VectorError,
                    format!(
                        "vector {:?} is profile-independent and exercises the method {:?}, which \
                         this profile does not declare; it does not apply and was not added to the \
                         corpus",
                        document.id, document.method
                    ),
                ));
                skipped.push((document.id, document.method));
                continue;
            }

            if !seen_ids.insert(document.id.clone()) {
                diagnostics.push(Diagnostic::error(
                    ErrorClass::VectorError,
                    format!(
                        "two vectors declare the identifier {:?}; a report naming it could not say \
                         which result it meant",
                        document.id
                    ),
                ));
            }

            for finding in validate::validate(bundle, &document).findings() {
                diagnostics.push(finding.clone());
            }

            vectors.push(Vector { path, document });
        }

        into_result(&diagnostics)?;

        Ok(Self {
            vectors,
            skipped,
            diagnostics,
        })
    }

    /// Every vector in the corpus, in the order they were read.
    #[must_use]
    pub fn vectors(&self) -> &[Vector] {
        &self.vectors
    }

    /// How many vectors the corpus holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Whether the corpus holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    /// The vector with the given identifier, if the corpus holds it.
    #[must_use]
    pub fn by_id(&self, id: &str) -> Option<&Vector> {
        self.vectors.iter().find(|vector| vector.id() == id)
    }

    /// The vectors that carry every one of `tags`.
    ///
    /// The selection mechanism CI uses to run a subset of a suite. It is a
    /// conjunctive filter rather than a disjunctive one because the tags a vector
    /// carries describe it along independent axes, and a caller asking for
    /// `authorization` and `negative` means both.
    #[must_use]
    pub fn matching(&self, tags: &[String]) -> Vec<&Vector> {
        self.vectors
            .iter()
            .filter(|vector| {
                tags.iter()
                    .all(|wanted| vector.document.tags.contains(wanted))
            })
            .collect()
    }

    /// The vectors that exercise the given method.
    #[must_use]
    pub fn for_method(&self, method: &str) -> Vec<&Vector> {
        self.vectors
            .iter()
            .filter(|vector| vector.document.method == method)
            .collect()
    }

    /// The profile-independent vectors that did not apply, as `(id, method)`.
    ///
    /// Exposed rather than merely warned about, because a suite that shrank is a
    /// fact the report should be able to state in a machine-readable way.
    #[must_use]
    pub fn skipped(&self) -> &[(String, String)] {
        &self.skipped
    }

    /// Findings that did not stop the corpus from loading.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.diagnostics.findings()
    }
}

/// Resolves a declared directory into the vector files it holds.
fn declared_tree(declared: &Path, what: &str, name: &str) -> Result<Vec<PathBuf>> {
    if !declared.is_dir() {
        return Err(Error::new(
            ErrorClass::VectorError,
            format!(
                "the profile declares {name:?} under {what}, but {} is not a directory; a declared \
                 requirement directory that does not exist is a suite that is missing vectors \
                 rather than a profile that does not claim them",
                declared.display()
            ),
        )
        .with_context("declared", name.to_owned())
        .with_context("path", declared.display().to_string()));
    }

    let found = yaml_files(declared)?;
    if found.is_empty() {
        return Err(Error::new(
            ErrorClass::VectorError,
            format!(
                "the profile declares {name:?} under {what}, but {} holds no vectors",
                declared.display()
            ),
        )
        .with_context("declared", name.to_owned()));
    }
    Ok(found)
}

/// Collects every `.yaml` file below `root`, bounded in count and depth.
fn yaml_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0_usize)];

    while let Some((directory, depth)) = stack.pop() {
        if depth > MAX_WALK_DEPTH {
            return Err(Error::new(
                ErrorClass::VectorError,
                format!(
                    "the vector tree below {} is deeper than {MAX_WALK_DEPTH} levels, which the \
                     layout does not reach",
                    root.display()
                ),
            ));
        }

        let entries = std::fs::read_dir(&directory).map_err(|problem| {
            Error::new(
                ErrorClass::VectorError,
                format!("{} could not be listed: {problem}", directory.display()),
            )
        })?;

        for entry in entries {
            let entry = entry.map_err(|problem| {
                Error::new(
                    ErrorClass::VectorError,
                    format!("{} could not be listed: {problem}", directory.display()),
                )
            })?;

            // The file type is taken from the directory entry rather than by
            // following the path, so a symlinked directory is neither descended
            // into nor treated as a vector. A tree that can reach outside itself
            // through a link is a tree whose corpus is not the one the profile
            // declared.
            let file_type = entry.file_type().map_err(|problem| {
                Error::new(
                    ErrorClass::VectorError,
                    format!(
                        "{} could not be inspected: {problem}",
                        entry.path().display()
                    ),
                )
            })?;

            let path = entry.path();
            if file_type.is_dir() {
                stack.push((path, depth + 1));
            } else if file_type.is_file()
                && path.extension().and_then(std::ffi::OsStr::to_str) == Some("yaml")
            {
                if found.len() >= MAX_VECTORS {
                    return Err(Error::new(
                        ErrorClass::VectorError,
                        format!(
                            "the vector tree below {} holds more than {MAX_VECTORS} vectors",
                            root.display()
                        ),
                    ));
                }
                found.push(path);
            }
        }
    }

    Ok(found)
}

/// Reads and parses one vector document, bounding how much is read.
fn read_yaml(path: &Path) -> Result<VectorDocument> {
    let metadata = std::fs::metadata(path).map_err(|problem| {
        Error::new(
            ErrorClass::VectorError,
            format!(
                "the vector at {} could not be read: {problem}",
                path.display()
            ),
        )
    })?;
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(Error::new(
            ErrorClass::VectorError,
            format!(
                "the vector at {} is {} bytes, above the {MAX_DOCUMENT_BYTES}-byte limit",
                path.display(),
                metadata.len()
            ),
        ));
    }

    let text = std::fs::read_to_string(path).map_err(|problem| {
        Error::new(
            ErrorClass::VectorError,
            format!(
                "the vector at {} could not be read as text: {problem}",
                path.display()
            ),
        )
    })?;

    serde_yaml_ng::from_str(&text).map_err(|problem| {
        Error::new(
            ErrorClass::VectorError,
            format!("the vector at {} is malformed: {problem}", path.display()),
        )
        .with_context("vector", path.display().to_string())
    })
}
