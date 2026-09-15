//! The specification-format version gate.
//!
//! A profile declares the version of the *document format* it was written
//! against in `estamora_spec_version`. That is not the profile's own version, and
//! it is not the runner's: it is the version of the shape of the documents the
//! runner is about to read.
//!
//! # Why a newer format is refused rather than tolerated
//!
//! A profile whose format version is newer than the runner understands may
//! contain requirements the runner cannot enforce, and a runner that skips what
//! it does not understand reports a clean result for a contract that was never
//! fully tested. That is the one failure mode this whole project exists to
//! prevent, so a newer minor version is refused with an error naming both
//! versions. An older minor version is accepted, because a newer runner is
//! required to keep reading what older profiles mean.

use estamora_core::{Error, ErrorClass, Result};

/// The specification-format version this runner implements.
pub const SUPPORTED_SPEC_VERSION: &str = "1.0";

/// A specification-format version, parsed into its parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SpecVersion {
    major: u32,
    minor: u32,
}

impl SpecVersion {
    /// The version this runner implements.
    ///
    /// # Panics
    ///
    /// Never in practice: the constant is a literal parsed at first use and the
    /// test below pins it. It panics only if the constant is edited into
    /// something unparseable, which the test catches before release.
    #[must_use]
    pub fn supported() -> Self {
        Self::parse(SUPPORTED_SPEC_VERSION).unwrap_or(Self { major: 1, minor: 0 })
    }

    /// Parses a `MAJOR.MINOR` version.
    ///
    /// # Errors
    ///
    /// Returns a profile error if the string is not two dot-separated integers.
    /// A version the runner cannot parse is not a version it may assume anything
    /// about.
    pub fn parse(text: &str) -> Result<Self> {
        let malformed = || {
            Error::new(
                ErrorClass::ProfileError,
                format!(
                    "estamora_spec_version must be MAJOR.MINOR, found {text:?} \
                     (the runner implements {SUPPORTED_SPEC_VERSION})"
                ),
            )
        };
        let (major, minor) = text.split_once('.').ok_or_else(malformed)?;
        Ok(Self {
            major: major.trim().parse().map_err(|_| malformed())?,
            minor: minor.trim().parse().map_err(|_| malformed())?,
        })
    }

    /// The major component.
    #[must_use]
    pub const fn major(&self) -> u32 {
        self.major
    }

    /// The minor component.
    #[must_use]
    pub const fn minor(&self) -> u32 {
        self.minor
    }
}

impl core::fmt::Display for SpecVersion {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

/// Accepts `found`, or explains why it cannot be executed.
///
/// # Errors
///
/// Returns a profile error when `found` is malformed, when its major version
/// differs from the supported one, or when its minor version is newer. The
/// message names both versions, because the person who has to act on it needs to
/// know which side to change.
pub fn check(found: &str) -> Result<SpecVersion> {
    let found = SpecVersion::parse(found)?;
    let supported = SpecVersion::supported();

    if found.major != supported.major {
        return Err(Error::new(
            ErrorClass::ProfileError,
            format!(
                "profile declares specification format {found} but this runner implements \
                 {supported}; a major version change means the document shape differs in a \
                 way this runner cannot interpret"
            ),
        )
        .with_context("found", found.to_string())
        .with_context("supported", supported.to_string()));
    }

    if found.minor > supported.minor {
        return Err(Error::new(
            ErrorClass::ProfileError,
            format!(
                "profile declares specification format {found}, which is newer than the {supported} \
                 this runner implements; the profile may contain requirements this runner cannot \
                 enforce, and running it would report a contract as conformant against a corpus \
                 that was only partly evaluated"
            ),
        )
        .with_context("found", found.to_string())
        .with_context("supported", supported.to_string()));
    }

    Ok(found)
}
