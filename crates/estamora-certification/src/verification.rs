//! Signing and verifying a receipt.
//!
//! # Verification answers two questions, not one
//!
//! 1. **Is the report the one this receipt is about?** Answered by recomputing the
//!    report's digest and comparing. This is checkable without any key, and it is the
//!    property that matters most: it is what stops a report being swapped for a
//!    better-looking one.
//! 2. **Who asserted it?** Answered by a signature, and *only* by a signature checked
//!    against a key the verifier already trusts. A signature checked against a key
//!    carried in the same document proves that the document is self-consistent and
//!    nothing more — anyone can generate a key pair and sign anything with it.
//!
//! Collapsing the two would produce the most misleading artefact this project could
//! publish: a "verified" receipt that attests to nothing but its own internal
//! agreement. So [`verify`] returns an [`Attribution`], and the unsigned case is a
//! value a caller has to look at rather than an error a caller can ignore.
//!
//! # Why the payload is the receipt's digest
//!
//! Signing a digest rather than a document means the signed bytes are a
//! fixed-length phrase whatever the receipt contains, so a verifier does not have to
//! reproduce the signature's serialisation, and a signature stays valid if a field
//! the digest does not cover is added later. The digest covers the content; the
//! signature covers the digest.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use estamora_core::{Error, ErrorClass, Result};
use serde::{Deserialize, Serialize};

use crate::digest::Digest;
use crate::receipt::{Receipt, report_digest};

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
    /// consistency, which is why [`verify`] reports whether it was told to.
    pub public_key: String,
    /// The signature over the receipt's digest, base64-encoded.
    pub signature: String,
    /// The receipt.
    pub receipt: Receipt,
}

/// Who a verified receipt is attributed to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribution {
    /// The signature was checked against a key the caller supplied and trusts.
    ///
    /// The key's fingerprint is reported so that a record can say which key
    /// asserted the result, which is the fact that makes a receipt accountable.
    Attributed {
        /// The trusted key the signature was checked against, as a fingerprint.
        key: String,
    },
    /// The receipt carried no signature, or the caller supplied no key to check it
    /// against.
    ///
    /// The report's digest has still been checked: the receipt is about the report
    /// shown to the verifier. What is missing is any evidence of who issued it.
    Unattributed {
        /// Why nothing is attributed.
        reason: String,
    },
}

impl Attribution {
    /// Whether the receipt was attributed to a trusted issuer.
    #[must_use]
    pub const fn is_attributed(&self) -> bool {
        matches!(self, Self::Attributed { .. })
    }
}

/// The outcome of verifying a receipt against a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    /// The digest of the report the receipt was checked against.
    pub report_digest: Digest,
    /// Who, if anyone, the receipt is attributed to.
    pub attribution: Attribution,
}

impl Verification {
    /// Whether the receipt is attributed to a trusted issuer.
    #[must_use]
    pub const fn is_attributed(&self) -> bool {
        self.attribution.is_attributed()
    }
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

/// Verifies a signed receipt against the report it claims to be about.
///
/// `trusted` is the key the caller already trusts for the issuer. Passing `None` is
/// legitimate and common — a caller comparing a report with a receipt does not need
/// an issuer — and it produces [`Attribution::Unattributed`] rather than failing,
/// because "the report matches and nobody is identified" is a real and useful
/// answer.
///
/// # Errors
///
/// Returns a certification error, with `reason` in the context, when:
///
/// - the report is not the one the receipt is about (`report-digest-mismatch`),
/// - the declared algorithm is not one that can be checked (`unsupported-algorithm`),
/// - a declared signature or key is not valid base64 or has the wrong length
///   (`malformed-signature`),
/// - a trusted key was supplied and the signature does not check out under it
///   (`signature-invalid`).
pub fn verify(
    signed: &SignedReceipt,
    report: &estamora_report::model::Report,
    trusted: Option<&VerifyingKey>,
) -> Result<Verification> {
    let actual = report_digest(report)?;
    if actual != signed.receipt.report_digest {
        return Err(Error::new(
            ErrorClass::CertificationError,
            format!(
                "the report is not the one this receipt is about: the receipt commits to {}, \
                 and the report shown hashes to {actual}",
                signed.receipt.report_digest
            ),
        )
        .with_context("reason", "report-digest-mismatch")
        .with_context("expected", signed.receipt.report_digest.to_string())
        .with_context("actual", actual.to_string()));
    }

    if signed.algorithm != ALGORITHM {
        return Err(Error::new(
            ErrorClass::CertificationError,
            format!(
                "the receipt declares the signature algorithm {:?}, and this runner can only \
                 check `{ALGORITHM}`",
                signed.algorithm
            ),
        )
        .with_context("reason", "unsupported-algorithm"));
    }

    // A signature with nothing to check it against is not an error: the report has
    // been matched, and the caller simply did not ask who issued it.
    let Some(trusted) = trusted else {
        return Ok(Verification {
            report_digest: actual,
            attribution: Attribution::Unattributed {
                reason: "no trusted key was supplied, so the signature was not checked; a \
                         signature checked against a key carried in the same document \
                         establishes only that the document agrees with itself"
                    .to_owned(),
            },
        });
    };

    let signature = decode_signature(&signed.signature)?;
    let payload = signed.receipt.signing_payload()?;
    trusted
        .verify_strict(payload.as_bytes(), &signature)
        .map_err(|problem| {
            Error::new(
                ErrorClass::CertificationError,
                format!("the signature does not check out under the trusted key: {problem}"),
            )
            .with_context("reason", "signature-invalid")
        })?;

    Ok(Verification {
        report_digest: actual,
        attribution: Attribution::Attributed {
            key: fingerprint(trusted),
        },
    })
}

/// Decodes a declared signature.
///
/// # Errors
///
/// Returns a certification error when it is not base64 or not sixty-four bytes.
fn decode_signature(encoded: &str) -> Result<Signature> {
    let bytes = BASE64.decode(encoded).map_err(|problem| {
        Error::new(
            ErrorClass::CertificationError,
            format!("the signature is not valid base64: {problem}"),
        )
        .with_context("reason", "malformed-signature")
    })?;
    let bytes: [u8; 64] = bytes.as_slice().try_into().map_err(|_| {
        Error::new(
            ErrorClass::CertificationError,
            format!(
                "an {ALGORITHM} signature is 64 bytes; found {}",
                bytes.len()
            ),
        )
        .with_context("reason", "malformed-signature")
    })?;
    Ok(Signature::from_bytes(&bytes))
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
    use super::{fingerprint, sign, signing_key_from_hex, verify, verifying_key_from_base64};
    use crate::receipt::Receipt;
    use crate::tests::a_report;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use ed25519_dalek::SigningKey;

    fn a_key() -> SigningKey {
        signing_key_from_hex(&"11".repeat(32)).unwrap()
    }

    #[test]
    fn a_signed_receipt_verifies_against_the_key_that_signed_it() {
        let report = a_report();
        let key = a_key();
        let receipt = Receipt::issue(&report, "2026-01-15T12:00:00Z").unwrap();
        let signed = sign(&receipt, &key).unwrap();
        let verification = verify(&signed, &report, Some(&key.verifying_key())).unwrap();
        assert!(verification.is_attributed());
        assert_eq!(
            verification.attribution,
            super::Attribution::Attributed {
                key: fingerprint(&key.verifying_key())
            }
        );
    }

    #[test]
    fn a_report_that_is_not_the_one_the_receipt_is_about_is_refused() {
        // The check that matters most, and the one that needs no key: a report
        // cannot be swapped for a better-looking one.
        let report = a_report();
        let key = a_key();
        let signed = sign(
            &Receipt::issue(&report, "2026-01-15T12:00:00Z").unwrap(),
            &key,
        )
        .unwrap();

        let mut other = a_report();
        other.status = estamora_core::ConformanceStatus::Conformant;
        other.exit_code = 0;

        let problem = verify(&signed, &other, Some(&key.verifying_key())).unwrap_err();
        assert_eq!(
            problem.context_value("reason"),
            Some("report-digest-mismatch")
        );
    }

    #[test]
    fn a_signature_from_another_key_is_refused() {
        let report = a_report();
        let signed = sign(
            &Receipt::issue(&report, "2026-01-15T12:00:00Z").unwrap(),
            &a_key(),
        )
        .unwrap();
        let other = signing_key_from_hex(&"22".repeat(32)).unwrap();
        let problem = verify(&signed, &report, Some(&other.verifying_key())).unwrap_err();
        assert_eq!(problem.context_value("reason"), Some("signature-invalid"));
    }

    #[test]
    fn a_receipt_that_carries_its_own_key_is_not_attributed_without_a_trusted_one() {
        // Anyone can generate a key and sign anything. A verifier that trusted the
        // key carried in the document would be verifying nothing, so the absence of a
        // trusted key is reported as an absence of attribution rather than as
        // success.
        let report = a_report();
        let signed = sign(
            &Receipt::issue(&report, "2026-01-15T12:00:00Z").unwrap(),
            &a_key(),
        )
        .unwrap();
        let verification = verify(&signed, &report, None).unwrap();
        assert!(!verification.is_attributed());
        match verification.attribution {
            super::Attribution::Unattributed { reason } => {
                assert!(reason.contains("no trusted key"), "{reason}");
            },
            super::Attribution::Attributed { .. } => panic!("nothing was trusted"),
        }
    }

    #[test]
    fn a_malformed_signature_is_reported_as_malformed_rather_than_as_a_failure_of_the_key() {
        let report = a_report();
        let key = a_key();
        let mut signed = sign(
            &Receipt::issue(&report, "2026-01-15T12:00:00Z").unwrap(),
            &key,
        )
        .unwrap();
        signed.signature = "not base64 at all !!".to_owned();
        let problem = verify(&signed, &report, Some(&key.verifying_key())).unwrap_err();
        assert_eq!(problem.context_value("reason"), Some("malformed-signature"));
    }

    #[test]
    fn an_unsupported_algorithm_is_named_rather_than_ignored() {
        let report = a_report();
        let key = a_key();
        let mut signed = sign(
            &Receipt::issue(&report, "2026-01-15T12:00:00Z").unwrap(),
            &key,
        )
        .unwrap();
        signed.algorithm = "rsa-pkcs1v15".to_owned();
        let problem = verify(&signed, &report, Some(&key.verifying_key())).unwrap_err();
        assert_eq!(
            problem.context_value("reason"),
            Some("unsupported-algorithm")
        );
    }

    #[test]
    fn a_key_is_read_from_its_documented_spelling_and_rejected_otherwise() {
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
