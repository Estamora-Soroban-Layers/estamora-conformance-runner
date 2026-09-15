//! Small helpers the renderers share.
//!
//! Both renderers accumulate into a `String`, and both need two things from that:
//! formatted appending that a lint is satisfied by, and a single place where an
//! interpolated value is made safe. Keeping them here rather than duplicating them
//! is why the two renderings cannot drift apart in how they treat untrusted text.

use std::fmt::{self, Write as _};

/// Appends formatted text.
///
/// Writing into a `String` cannot fail — its error type exists for writers that can
/// — so the result is discarded rather than propagated. The helper exists so that a
/// renderer reads as a sequence of appends rather than as a sequence of
/// `push_str(&format!(…))` calls.
pub fn push(out: &mut String, arguments: fmt::Arguments<'_>) {
    let _ignored = out.write_fmt(arguments);
}

/// XML-escapes a value.
///
/// A concrete `&str` parameter rather than a generic one, so that a caller passing
/// an owned string does not have to borrow it explicitly.
#[must_use]
pub fn xml(value: &str) -> String {
    quick_xml::escape::escape(value).into_owned()
}
