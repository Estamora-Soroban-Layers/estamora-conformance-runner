//! Loading a profile bundle from disk.
//!
//! A bundle is a directory, not a file: `profile.yaml` declares the other
//! documents through its manifest, and the loader resolves them. Everything here
//! is written on the assumption that a bundle is input from outside this
//! repository and may be wrong in any way, including hostilely.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use estamora_core::{Diagnostic, Diagnostics, Error, ErrorClass, Result, into_result};
use serde::de::DeserializeOwned;

use crate::spec::{self, SpecVersion};
use crate::types::{MethodsDocument, ProfileDocument};

/// The document that declares a bundle.
pub const BUNDLE_ENTRY_POINT: &str = "profile.yaml";

/// The largest document the loader will read, in bytes.
///
/// A profile is hand-written text. A document beyond this bound is either a
/// mistake or an attempt to make the runner allocate without limit, and either
/// way reading it is not going to produce a verdict about a contract.
pub const MAX_DOCUMENT_BYTES: u64 = 8 * 1024 * 1024;

/// `id@version`, the way a profile is named in a command line, a report and a
/// receipt.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProfileReference {
    /// The profile identifier.
    pub id: String,
    /// The profile version.
    pub version: String,
}

impl ProfileReference {
    /// Builds a reference from its parts.
    #[must_use]
    pub fn new(id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
        }
    }

    /// Parses `id@version`.
    ///
    /// # Errors
    ///
    /// Returns a profile error if the `@` is missing or either side is empty. A
    /// reference with no version is not a reference: it names whatever the
    /// current revision happens to be, which is not a stable statement about
    /// anything.
    pub fn parse(text: &str) -> Result<Self> {
        let (id, version) = text.split_once('@').ok_or_else(|| {
            Error::new(
                ErrorClass::ProfileError,
                format!(
                    "a profile reference is written ID@VERSION, found {text:?}; a reference with no \
                     version names whatever the current revision happens to be"
                ),
            )
        })?;
        if id.is_empty() || version.is_empty() {
            return Err(Error::new(
                ErrorClass::ProfileError,
                format!(
                    "a profile reference must name both an identifier and a version, found {text:?}"
                ),
            ));
        }
        Ok(Self::new(id, version))
    }
}

impl fmt::Display for ProfileReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.id, self.version)
    }
}

/// A loaded profile bundle.
///
/// Holding the parsed entry point and the path together is deliberate: every
/// other document is resolved relative to the bundle that declared it, so a
/// bundle cannot be read from one place and its documents from another.
#[derive(Debug, Clone)]
pub struct ProfileBundle {
    root: PathBuf,
    document: ProfileDocument,
    methods: MethodsDocument,
    spec_version: SpecVersion,
    diagnostics: Diagnostics,
}

impl ProfileBundle {
    /// Loads the bundle rooted at `root`.
    ///
    /// # Errors
    ///
    /// Returns a profile error if the root is not a directory, if the entry
    /// point is missing, unreadable, empty, oversized, malformed, or declares a
    /// document that does not exist, escapes the bundle, or is not a regular
    /// file. Also returns one if the declared specification-format version is
    /// not one this runner can execute.
    pub fn load(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        if !root.is_dir() {
            return Err(Error::new(
                ErrorClass::ProfileError,
                format!(
                    "a profile bundle is a directory, but {} is not one",
                    root.display()
                ),
            )
            .with_context("path", root.display().to_string()));
        }

        let entry = root.join(BUNDLE_ENTRY_POINT);
        let document: ProfileDocument = read_yaml(&entry, "the profile document")?;
        let spec_version = spec::check(&document.estamora_spec_version)?;

        let mut diagnostics = Diagnostics::new();
        for (role, file) in document.includes.files() {
            resolve_document(&root, role, file)?;
        }

        // The method document is parsed now rather than on demand. A bundle that
        // is malformed must be refused before anything is executed, and a
        // document parsed lazily is a document whose defects surface after the
        // run has already begun.
        //
        // The other five documents are checked for existence only, because the
        // typed model for their contents does not exist yet. That is a limitation
        // rather than a decision, it is narrowed as each document is modelled,
        // and it is stated here so that it is a known gap rather than a silent
        // one: until they are modelled, a profile that is semantically wrong in
        // those documents is caught by the specification repository's own
        // validation, not by the runner.
        let methods: MethodsDocument = read_yaml(
            &root.join(&document.includes.methods),
            "the method requirements",
        )?;

        check_layout(&root, &document, &mut diagnostics);

        // A bundle stored under one identity while declaring another is fatal
        // rather than a warning: a consumer that resolves a profile by path must
        // not silently execute a different one. Anything left in the collection
        // at this point is a warning, and a warning is reported rather than
        // acted on.
        into_result(&diagnostics)?;

        Ok(Self {
            root,
            document,
            methods,
            spec_version,
            diagnostics,
        })
    }

    /// The directory this bundle was loaded from.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The parsed entry point.
    #[must_use]
    pub fn document(&self) -> &ProfileDocument {
        &self.document
    }

    /// The reference this bundle is named by.
    #[must_use]
    pub fn reference(&self) -> ProfileReference {
        ProfileReference::new(
            self.document.profile.id.clone(),
            self.document.profile.version.clone(),
        )
    }

    /// The specification-format version this bundle declared, as parsed.
    #[must_use]
    pub const fn spec_version(&self) -> SpecVersion {
        self.spec_version
    }

    /// The specification-format version exactly as the document wrote it.
    #[must_use]
    pub fn spec_version_text(&self) -> &str {
        &self.document.estamora_spec_version
    }

    /// Findings that do not stop the bundle from loading.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.diagnostics.findings()
    }

    /// The method requirements this bundle declares.
    ///
    /// Parsed during [`ProfileBundle::load`], so reaching this point means the
    /// document was readable and well-formed.
    #[must_use]
    pub const fn methods(&self) -> &MethodsDocument {
        &self.methods
    }
}

/// Resolves one manifest entry, refusing anything that leaves the bundle.
fn resolve_document(root: &Path, role: &str, file: &str) -> Result<PathBuf> {
    let declared = Path::new(file);

    // A manifest names a file inside its own bundle. A path that is absolute, or
    // that climbs out with `..`, either reads something the bundle was not
    // supposed to see or reads something that is not there, and neither is a
    // profile the runner should execute.
    let escapes = declared.is_absolute()
        || declared.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
    if escapes {
        return Err(Error::new(
            ErrorClass::ProfileError,
            format!("the {role} document is declared as {file:?}, which points outside the bundle"),
        )
        .with_context("bundle", root.display().to_string())
        .with_context("document", file.to_owned()));
    }

    let path = root.join(declared);
    let metadata = std::fs::metadata(&path).map_err(|problem| {
        Error::new(
            ErrorClass::ProfileError,
            format!(
                "the bundle declares a {role} document at {file:?} and it could not be read: {problem}"
            ),
        )
        .with_context("bundle", root.display().to_string())
        .with_context("document", file.to_owned())
    })?;
    if !metadata.is_file() {
        return Err(Error::new(
            ErrorClass::ProfileError,
            format!("the {role} document at {file:?} is not a regular file"),
        )
        .with_context("bundle", root.display().to_string()));
    }

    Ok(path)
}

/// Checks that a bundle sits where its own identity says it should.
///
/// The canonical layout is `profiles/<id>/<version>/`, so a directory whose name
/// equals the declared version sits where the parent is expected to name the
/// profile. When it does not, the bundle is reachable at the path of one profile
/// while declaring another, which is how a consumer resolves `sep-41@1.0` and
/// silently executes something else. That is an error.
///
/// A bundle in a directory that is not named for a version at all — a worked
/// example under `profiles/examples/`, for instance — has nothing to compare
/// against and is reported as a warning instead: it is legitimately loadable, and
/// it is worth saying that resolving it by path will not find it.
fn check_layout(root: &Path, document: &ProfileDocument, diagnostics: &mut Diagnostics) {
    let declared_version = &document.profile.version;
    let declared_id = &document.profile.id;
    let directory = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let parent = root
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if directory == declared_version {
        if parent != declared_id {
            diagnostics.push(Diagnostic::error(
                ErrorClass::ProfileError,
                format!(
                    "the bundle is stored under {parent}/{directory} but declares \
                     {declared_id}@{declared_version}; resolving {declared_id}@{declared_version} \
                     by path would not reach this directory"
                ),
            ));
        }
        return;
    }

    diagnostics.warning(
        ErrorClass::ProfileError,
        format!(
            "the bundle is at {} rather than profiles/<id>/<version>/; a consumer resolving \
             {} by path would not find it here",
            root.display(),
            ProfileReference::new(declared_id.clone(), declared_version.clone())
        ),
    );
}

/// Reads and parses one YAML document, bounding how much is read.
fn read_yaml<T: DeserializeOwned>(path: &Path, what: &str) -> Result<T> {
    let metadata = std::fs::metadata(path).map_err(|problem| {
        Error::new(
            ErrorClass::ProfileError,
            format!("{what} at {} could not be read: {problem}", path.display()),
        )
    })?;
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(Error::new(
            ErrorClass::ProfileError,
            format!(
                "{what} at {} is {} bytes, above the {MAX_DOCUMENT_BYTES}-byte limit",
                path.display(),
                metadata.len()
            ),
        ));
    }

    let text = std::fs::read_to_string(path).map_err(|problem| {
        Error::new(
            ErrorClass::ProfileError,
            format!(
                "{what} at {} could not be read as text: {problem}",
                path.display()
            ),
        )
    })?;

    serde_yaml_ng::from_str(&text).map_err(|problem| {
        // The parser's message already carries the line and column, which is the
        // part that makes a malformed profile fixable.
        Error::new(
            ErrorClass::ProfileError,
            format!("{what} at {} is malformed: {problem}", path.display()),
        )
        .with_context("document", path.display().to_string())
    })
}
