//! Exact integers, written as strings.
//!
//! A token amount is an `i128`, and an `i128` does not survive a round trip
//! through a double-precision float. A format that wrote `250` as a number would
//! hand the runner whatever the parser's float handling produced, which for a
//! value near the limit is a different number — and the difference would appear
//! only in the expected balance of an assertion that is supposed to catch a
//! one-unit error.
//!
//! So every quantity in the format is a string, and this type converts it to an
//! integer **once, at load time**, rejecting anything that is not exactly a
//! canonical decimal integer. Doing it here rather than at each use site is what
//! stops a malformed amount from reaching an assertion as a plausible zero.

use std::fmt;

use serde::de::{self, Deserialize, Deserializer};
use serde_yaml_ng::Value;

/// An exact integer, parsed from the string form the format requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct IntegerString(i128);

impl IntegerString {
    /// Builds one from an integer.
    #[must_use]
    pub const fn new(value: i128) -> Self {
        Self(value)
    }

    /// The value.
    #[must_use]
    pub const fn get(self) -> i128 {
        self.0
    }
}

impl fmt::Display for IntegerString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Whether `text` is exactly `0`, or an optional minus followed by a canonical
/// decimal with no leading zero.
///
/// `-0` is rejected: it is a distinct spelling of zero, and a format that accepted
/// two spellings of the same value would make two otherwise identical vectors
/// differ textually while asserting the same thing.
fn is_canonical_integer(text: &str) -> bool {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    if negative {
        !digits.starts_with('0')
    } else {
        digits == "0" || !digits.starts_with('0')
    }
}

impl<'de> Deserialize<'de> for IntegerString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // The YAML node is inspected rather than deserialized into a `String`
        // directly, and that is the whole point of this implementation. A YAML
        // parser asked for a string will happily stringify a scalar it has already
        // classified as a number, so `1000` would arrive as `"1000"` and be
        // accepted — which is fine — while `0777` would arrive as `"511"`, having
        // been read as an octal literal and re-spelled in decimal. The document
        // would say 777, the runner would assert 511, and no error would be
        // raised anywhere. Requiring the node to be a string makes that class of
        // silent re-interpretation impossible.
        let node = Value::deserialize(deserializer)?;
        let text = match node {
            Value::String(text) => text,
            other => {
                return Err(de::Error::custom(format!(
                    "an exact integer must be written as a quoted string, so that no scalar parser \
                     ever classifies it; found {other:?}"
                )));
            },
        };
        if !is_canonical_integer(&text) {
            return Err(de::Error::custom(format!(
                "{text:?} is not a canonical decimal integer; the format is 0 or an optional minus \
                 followed by digits with no leading zero"
            )));
        }
        let value = text.parse::<i128>().map_err(|_| {
            de::Error::custom(format!(
                "{text:?} is outside the range of a signed 128-bit integer"
            ))
        })?;
        Ok(Self(value))
    }
}
