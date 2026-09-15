//! Producing signatures, and reading the keys that go with them.
//!
//! # What a signature does and does not establish
//!
//! Signing a receipt records *who asserted a result*. It says nothing about whether
//! the result is correct: a valid signature over a wrong conclusion is still a wrong
//! conclusion, and no amount of key handling changes that. The other half of this
//! pair, [`crate::verification`], is where that distinction is enforced.
//!
//! # Why a public key travels with the signature
//!
//! [`SignedReceipt::public_key`] is carried so that a signature can be *identified* —
//! two issuers in one project can be told apart by [`fingerprint`] — and not so that it
//! can be trusted. Anyone can generate a key pair and sign anything with it, so a
//! signature checked against the key inside the same document establishes only that the
//! document agrees with itself. That is why verification takes the trusted key as an
//! argument rather than reading it out of the receipt.
//!
//! # Why the payload is the receipt's digest
//!
//! Signing a digest rather than a document means the signed bytes are a fixed-length
//! phrase whatever the receipt contains, so a verifier does not have to reproduce the
//! signature's serialisation, and a signature stays valid if a field the digest does not
//! cover is added later. The digest covers the content; the signature covers the digest.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use estamora_core::{Error, ErrorClass, Result};
use serde::{Deserialize, Serialize};

use crate::digest::Digest;
use crate::receipt::Receipt;

/// The only signature algorithm a receipt may declare.
pub const ALGORITHM: &str = "ed25519";

/// A receipt together with the signature that asserts it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedReceipt {
    /// `ed25519`, named explicitly so that a verifier never has to guess.
    pub algorithm: String,
    /// The signing key's public half, base64-encoded.
    ///
    /// Carried so that a signature can be identified, **not** so that it can be
    /// trusted. A verifier that trusts this field has verified nothing but internal
    /// consistency, which is why [`crate::verify`] reports whether it was told to.
    pub public_key: String,
    /// The signature over the receipt's digest, base64-encoded.
    pub signature: String,
    /// The receipt.
    pub receipt: Receipt,
}

/// Signs a receipt.
///
/// # Errors
///
/// Returns a certification error when the receipt cannot be digested.
pub fn sign(receipt: &Receipt, key: &SigningKey) -> Result<SignedReceipt> {
    let payload = receipt.signing_payload()?;
    let signature: Signature = key.sign(payload.as_bytes());
    Ok(SignedReceipt {
        algorithm: ALGORITHM.to_owned(),
        public_key: BASE64.encode(key.verifying_key().to_bytes()),
        signature: BASE64.encode(signature.to_bytes()),
        receipt: receipt.clone(),
    })
}

/// A short, stable name for a key.
///
/// The first sixteen hexadecimal characters of the digest of the key's bytes. Short
/// enough to quote in a report, long enough that two keys in one project never
/// collide.
#[must_use]
pub fn fingerprint(key: &VerifyingKey) -> String {
    let digest = Digest::of_bytes(key.as_bytes());
    digest.hex().chars().take(16).collect()
}

/// Reads a signing key from its 32-byte seed, hex-encoded.
///
/// # Errors
///
/// Returns a certification error when the input is not a 32-byte hexadecimal seed.
/// Accepting a shorter seed would silently be a different key than the operator
/// intended, and every receipt it produced would verify against a key nobody chose.
pub fn signing_key_from_hex(seed: &str) -> Result<SigningKey> {
    let bytes = hex::decode(seed).map_err(|problem| {
        Error::new(
            ErrorClass::CertificationError,
            format!("a signing key seed is hexadecimal: {problem}"),
        )
    })?;
    let bytes: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
        Error::new(
            ErrorClass::CertificationError,
            format!("a signing key seed is 32 bytes; found {}", bytes.len()),
        )
    })?;
    Ok(SigningKey::from_bytes(&bytes))
}

/// Reads a public key from its 32 bytes, base64-encoded.
///
/// # Errors
///
/// Returns a certification error when the input is not valid base64 or not 32 bytes.
pub fn verifying_key_from_base64(encoded: &str) -> Result<VerifyingKey> {
    let bytes = BASE64.decode(encoded).map_err(|problem| {
        Error::new(
            ErrorClass::CertificationError,
            format!("a public key is base64-encoded: {problem}"),
        )
    })?;
    let bytes: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
        Error::new(
            ErrorClass::CertificationError,
            format!("a public key is 32 bytes; found {}", bytes.len()),
        )
    })?;
    VerifyingKey::from_bytes(&bytes).map_err(|problem| {
        Error::new(
            ErrorClass::CertificationError,
            format!("the value is not a valid {ALGORITHM} public key: {problem}"),
        )
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{fingerprint, signing_key_from_hex, verifying_key_from_base64};
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;

    fn a_key() -> ed25519_dalek::SigningKey {
        signing_key_from_hex(&"11".repeat(32)).unwrap()
    }

    #[test]
    fn a_key_is_read_from_its_documented_spelling_and_rejected_otherwise() {
        // Accepting a short seed would silently be a different key than the operator
        // intended, and every receipt it produced would verify against a key nobody chose.
        let key = a_key();
        let encoded = BASE64.encode(key.verifying_key().to_bytes());
        assert_eq!(
            verifying_key_from_base64(&encoded).unwrap().to_bytes(),
            key.verifying_key().to_bytes()
        );
        assert!(verifying_key_from_base64("short").is_err());
        assert!(signing_key_from_hex("0011").is_err());
    }

    #[test]
    fn a_fingerprint_is_short_enough_to_quote_and_does_not_collide() {
        let first = fingerprint(&a_key().verifying_key());
        let second = fingerprint(
            &signing_key_from_hex(&"22".repeat(32))
                .unwrap()
                .verifying_key(),
        );
        assert_eq!(first.len(), 16);
        assert_ne!(first, second);
    }
}
