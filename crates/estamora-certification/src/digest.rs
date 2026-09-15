//! Content digests.
//!
//! A receipt commits to digests rather than to documents, because the documents are
//! large and the commitment is what matters: a verifier recomputes a digest from the
//! document it was handed and compares. If the two differ, the document is not the
//! one the receipt is about — which is a stronger and more useful statement than
//! "the signature is invalid", and it is why the comparison is done in that order.
//!
//! # The spelling is a cross-repository contract
//!
//! A digest is written `sha256:<64 lowercase hex characters>`, which is the pattern
//! the specification's profile schema fixes and the report schema refers to. The
//! type refuses to be constructed from anything else, so a digest that reaches a
//! report or a receipt is one a consumer's schema will accept.
//!
//! # Canonical JSON
//!
//! A digest over pretty-printed JSON would change if a formatter's policy did, so two
//! runs that reached the same result would stop verifying against each other. The
//! report's digest is therefore taken over its compact rendering, which
//! `estamora_report::render_json(_, false)` produces and a test pins.

use std::fmt;

use estamora_core::{Error, ErrorClass, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// The prefix every digest carries.
pub const PREFIX: &str = "sha256:";

/// The number of hexadecimal characters in a digest.
pub const HEX_LENGTH: usize = 64;

/// A content digest, as `sha256:<hex>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Digest(String);

impl Digest {
    /// The digest of a byte string.
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self(format!("{PREFIX}{}", hex::encode(hasher.finalize())))
    }

    /// The digest of a JSON document, over its canonical compact rendering.
    ///
    /// # Errors
    ///
    /// Returns a certification error if the value cannot be written as JSON, which
    /// would mean the value is not one a report could carry.
    pub fn of_json(value: &serde_json::Value) -> Result<Self> {
        let canonical = serde_json::to_vec(value).map_err(|problem| {
            Error::new(
                ErrorClass::CertificationError,
                format!("the document could not be canonicalised for digesting: {problem}"),
            )
        })?;
        Ok(Self::of_bytes(&canonical))
    }

    /// Reads a digest from its spelling.
    ///
    /// # Errors
    ///
    /// Returns a certification error when the spelling does not match the pattern
    /// the specification fixes. Refusing here is what keeps a malformed digest from
    /// reaching a document a consumer has to validate.
    pub fn parse(text: &str) -> Result<Self> {
        let Some(hex_part) = text.strip_prefix(PREFIX) else {
            return Err(Error::new(
                ErrorClass::CertificationError,
                format!("a digest is written as `{PREFIX}<hex>`; found {text:?}"),
            )
            .with_context("expected_prefix", PREFIX));
        };
        let acceptable = hex_part.len() == HEX_LENGTH
            && hex_part
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if !acceptable {
            return Err(Error::new(
                ErrorClass::CertificationError,
                format!(
                    "a digest carries {HEX_LENGTH} lowercase hexadecimal characters, and \
                     {hex_part:?} does not"
                ),
            ));
        }
        Ok(Self(text.to_owned()))
    }

    /// The digest's spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The hexadecimal portion.
    #[must_use]
    pub fn hex(&self) -> &str {
        self.0.strip_prefix(PREFIX).unwrap_or(&self.0)
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for Digest {
    type Error = Error;

    fn try_from(text: String) -> Result<Self> {
        Self::parse(&text)
    }
}

impl From<Digest> for String {
    fn from(digest: Digest) -> Self {
        digest.0
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::Digest;

    #[test]
    fn the_digest_is_standard_sha256_and_not_merely_self_consistent() {
        // The published digest of the empty string. A hash function that produced a
        // consistent but non-standard value would satisfy every round-trip test in
        // this crate and would not interoperate with anything.
        assert_eq!(
            Digest::of_bytes(b"").as_str(),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // And of "abc", so that a trivially-constant implementation is ruled out too.
        assert_eq!(
            Digest::of_bytes(b"abc").as_str(),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_digest_round_trips_through_its_spelling() {
        let digest = Digest::of_bytes(b"payload");
        assert_eq!(Digest::parse(digest.as_str()).unwrap(), digest);
        assert_eq!(digest.hex().len(), 64);
    }

    #[test]
    fn a_digest_that_is_not_sha256_hex_is_refused() {
        for malformed in [
            "deadbeef",
            "sha256:DEADBEEF",
            "sha256:short",
            &format!("sha512:{}", "0".repeat(64)),
            &format!("sha256:{}", "0".repeat(63)),
        ] {
            assert!(
                Digest::parse(malformed).is_err(),
                "{malformed} must not parse as a digest"
            );
        }
    }

    #[test]
    fn canonical_json_hashing_ignores_key_order_and_whitespace() {
        // `serde_json::Value` holds a map, so two documents that differ only in key
        // order are the same document — which is the property a digest over
        // "canonical JSON" is supposed to have.
        let first: serde_json::Value = serde_json::from_str("{\"a\":1,\"b\":[1,2]}").unwrap();
        let second: serde_json::Value =
            serde_json::from_str("{\n  \"b\": [1, 2],\n  \"a\": 1\n}\n").unwrap();
        assert_eq!(
            Digest::of_json(&first).unwrap(),
            Digest::of_json(&second).unwrap()
        );
    }
}
