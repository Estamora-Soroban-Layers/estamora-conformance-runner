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

use crate::authorization::AuthorizationDocument;
use crate::behavior::BehaviorDocument;
use crate::documents::ProfileDocuments;
use crate::events::EventsDocument;
use crate::failures::FailuresDocument;
use crate::invariants::InvariantsDocument;
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
/// Holding the parsed entry point, the six documents and the path together is
/// deliberate: every other document is resolved relative to the bundle that
/// declared it, so a bundle cannot be read from one place and its documents from
/// another, and a vector cannot be attributed to a profile that was loaded from
/// somewhere else.
#[derive(Debug, Clone)]
pub struct ProfileBundle {
    root: PathBuf,
    document: ProfileDocument,
    documents: ProfileDocuments,
    spec_version: SpecVersion,
    diagnostics: Diagnostics,
}

impl ProfileBundle {
    /// Loads the bundle rooted at `root`.
    ///
    /// # Errors
    ///
    /// Returns a profile error if the root is not a directory, if the entry point
    /// is missing, unreadable, empty, oversized or malformed, if it declares a
    /// document that does not exist, escapes the bundle or is not a regular file,
    /// if any of the six documents fails to parse, if any cross-reference between
    /// them does not resolve, if the declared specification-format version is not
    /// one this runner can execute, or if the bundle is stored under an identity
    /// other than the one it declares.
    ///
    /// Warnings do not stop the load. They are retained and available through
    /// [`ProfileBundle::diagnostics`], because a finding that does not prevent
    /// execution is still something the report should carry.
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

        // Every document is parsed now, not on demand. A bundle that is malformed
        // must be refused before anything is executed, and a document parsed
        // lazily is one whose defects surface only once the run has already begun
        // acting on the profile. The manifest's `files()` accessor is the single
        // list of what a bundle must declare, so a seventh document cannot be
        // added to the format and silently skipped here.
        let manifest = &document.includes;
        let documents = ProfileDocuments {
            methods: read_declared::<MethodsDocument>(
                &root,
                "methods",
                &manifest.methods,
                "the method requirements",
            )?,
            authorization: read_declared::<AuthorizationDocument>(
                &root,
                "authorization",
                &manifest.authorization,
                "the authorization requirements",
            )?,
            events: read_declared::<EventsDocument>(
                &root,
                "events",
                &manifest.events,
                "the event requirements",
            )?,
            behavior: read_declared::<BehaviorDocument>(
                &root,
                "behavior",
                &manifest.behavior,
                "the behavioural rules",
            )?,
            invariants: read_declared::<InvariantsDocument>(
                &root,
                "invariants",
                &manifest.invariants,
                "the invariants",
            )?,
            failures: read_declared::<FailuresDocument>(
                &root,
                "failures",
                &manifest.failures,
                "the failure requirements",
            )?,
        };

        let mut diagnostics = Diagnostics::new();
        check_layout(&root, &document, &mut diagnostics);

        // Cross-references are checked here rather than left to the
        // specification repository's validator, because a bundle reached by path
        // may not be the revision that was validated. A broken reference is an
        // error rather than a warning: a requirement that names something the
        // profile does not declare cannot be evaluated, and the runner must not
        // report a verdict against a requirement it silently did not apply.
        for finding in documents.validate().findings() {
            diagnostics.push(finding.clone());
        }

        // Anything left in the collection at this point is a warning, and a
        // warning is reported rather than acted on.
        into_result(&diagnostics)?;

        Ok(Self {
            root,
            document,
            documents,
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

    /// Every parsed document the bundle declares.
    #[must_use]
    pub const fn documents(&self) -> &ProfileDocuments {
        &self.documents
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
    #[must_use]
    pub const fn methods(&self) -> &MethodsDocument {
        &self.documents.methods
    }

    /// The operation directories the bundle owns vectors for.
    #[must_use]
    pub fn vector_directories(&self) -> &[String] {
        &self.document.includes.vectors
    }

    /// The shared vector families the bundle consumes.
    #[must_use]
    pub fn shared_vector_families(&self) -> &[String] {
        &self.document.includes.shared_vectors
    }
}

/// Resolves a declared document and parses it.
fn read_declared<T: DeserializeOwned>(
    root: &Path,
    role: &str,
    file: &str,
    what: &str,
) -> Result<T> {
    let path = resolve_document(root, role, file)?;
    read_yaml(&path, what)
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
