//! Normalising an internal identifier to the grammar the report schema accepts.
//!
//! The assertion layer names its checks after what they are — `behavior/<rule>/
//! postcondition/<n>`, `interface/method/<method>` — because a failure should say
//! what it was about. The report schema constrains an assertion identifier to a
//! lowercase kebab-case identifier of at most sixty-four characters, because an
//! identifier is a namespace other tools match on.
//!
//! Both are right, and they cannot both be satisfied by one string. So the
//! structure is normalised on the way out and the original is preserved in the
//! detail field: the document is valid for a consumer, and the check is still
//! locatable by a person.
//!
//! # Why a hash and not a plain truncation
//!
//! Two long identifiers that share a prefix — `behavior/transfer-…/postcondition/0`
//! and `…/1` — would truncate to the same string, and two distinct checks reported
//! under one identifier is a report that cannot be summarised or diffed. The
//! truncation therefore appends a digest of the original, which makes the mapping
//! injective while staying deterministic: the same check always produces the same
//! identifier, so two runs of the same corpus can be compared.

use sha2::{Digest as _, Sha256};

/// The longest identifier the schema permits.
pub const IDENTIFIER_LIMIT: usize = 64;

/// The shortest identifier the schema permits.
pub const IDENTIFIER_MINIMUM: usize = 2;

/// How many digest characters are appended when a name has to be cut.
const DISCRIMINATOR: usize = 8;

/// Normalises an internal identifier to the schema's grammar.
#[must_use]
pub fn identifier(raw: &str) -> String {
    let slug = slugify(raw);
    if slug.len() <= IDENTIFIER_LIMIT {
        return pad(slug);
    }
    let keep = IDENTIFIER_LIMIT - DISCRIMINATOR - 1;
    let mut truncated: String = slug.chars().take(keep).collect();
    truncated.push('-');
    truncated.push_str(&discriminator(raw));
    truncated
}

/// Reduces a string to lowercase kebab-case.
fn slugify(raw: &str) -> String {
    let mut slug = String::with_capacity(raw.len());
    let mut previous_was_separator = true;
    for character in raw.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !previous_was_separator {
            slug.push('-');
            previous_was_separator = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

/// Extends a slug that is too short to be an identifier.
///
/// A one-character name is not something a profile author writes, but it can be
/// derived — a normalised dot-separated code such as `x` — and emitting a document
/// the schema rejects is worse than lengthening a name.
fn pad(mut slug: String) -> String {
    if slug.len() >= IDENTIFIER_MINIMUM {
        return slug;
    }
    let digest = discriminator(&slug);
    slug.push('-');
    slug.push_str(&digest);
    slug
}

/// A deterministic discriminator over the original text.
fn discriminator(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    let digest = hex::encode(hasher.finalize());
    digest.chars().take(DISCRIMINATOR).collect()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{IDENTIFIER_LIMIT, identifier};

    /// The schema's pattern, so that the test checks the grammar rather than the
    /// normaliser's own idea of it.
    fn is_acceptable(id: &str) -> bool {
        let length = id.len();
        if !(2..=IDENTIFIER_LIMIT).contains(&length) {
            return false;
        }
        let mut characters = id.chars().peekable();
        loop {
            let segment: String = characters
                .by_ref()
                .take_while(|character| *character != '-')
                .collect();
            if segment.is_empty()
                || !segment
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            {
                return false;
            }
            if characters.peek().is_none() {
                return true;
            }
        }
    }

    #[test]
    fn a_structured_name_becomes_a_plain_identifier() {
        assert_eq!(
            identifier("interface/method/transfer"),
            "interface-method-transfer"
        );
        assert!(is_acceptable(&identifier("interface/method/transfer")));
    }

    #[test]
    fn a_long_name_is_cut_and_made_distinct() {
        let prefix = "behavior/transfer-without-authorization-fails/postcondition";
        let first = identifier(&format!("{prefix}/0"));
        let second = identifier(&format!("{prefix}/1"));
        assert!(is_acceptable(&first), "{first} must satisfy the grammar");
        assert!(is_acceptable(&second));
        assert_ne!(
            first, second,
            "two distinct checks must not share an identifier"
        );
    }

    #[test]
    fn normalisation_is_deterministic() {
        let raw = "behavior/transfer-without-authorization-fails/postcondition/0";
        assert_eq!(identifier(raw), identifier(raw));
    }

    #[test]
    fn a_dotted_code_becomes_kebab_case() {
        assert_eq!(
            identifier("invariant-warning.balances-ok"),
            "invariant-warning-balances-ok"
        );
    }
}
