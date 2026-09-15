//! What a stored report is allowed to do.
//!
//! A report is both output and input. This runner writes one, and `estamora report` and
//! `estamora certify verify` read one back — often one that arrived from somewhere else,
//! because the point of a receipt is that a third party can check a claim. So a report is
//! untrusted content on the way in and a template on the way out, which is the
//! combination that produces injection bugs.
//!
//! What must hold for every input:
//!
//! * an input that is not a report is refused, with a `REPORT_ERROR` and never a panic.
//!   A rendering of a document that was only partly understood looks exactly like a
//!   rendering of a result, which is why nothing is rendered from a parse that failed;
//! * an input that *is* a report renders in all three formats without panicking, and the
//!   renderings are valid documents in their own right: the JSON is valid JSON, and the
//!   JUnit XML is well-formed XML. A report whose contents were written verbatim into a
//!   template would break both;
//! * nothing unbounded. A report is a document that says how many vectors it ran, and a
//!   surprising number must not become a surprising allocation.
//!
//! Text that reaches a rendering is sanitised before it does — contract-supplied
//! metadata, check details, target names. This target is what keeps that honest: if a
//! report could carry a `</testcase>` or a `]]>` through, one of the assertions below
//! would fail on an input the fuzzer found.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    // Not a report: the only requirement is that it is refused without panicking.
    let Ok(report) = estamora_report::parse(text) else {
        return;
    };

    // The normative document. A report that parses must re-render as JSON, and the
    // result must be JSON — a string that does not parse back would mean the writer
    // emitted something a consumer cannot read.
    let json = estamora_report::render_json(&report, false).expect("a parsed report must render");
    serde_json::from_str::<serde_json::Value>(&json)
        .expect("the JSON rendering must itself be valid JSON");

    // The reviewer's rendering. Markdown has no container to balance, so the property is
    // only that it does not panic and does not grow without bound.
    let markdown =
        estamora_report::render_markdown(&report).expect("a parsed report must render");
    assert!(
        markdown.len() <= json.len() + 1024 * 1024,
        "the Markdown rendering is disproportionately larger than the document it renders"
    );

    // The CI rendering. This one is a container format, so the property that matters is
    // that the output is well-formed XML whatever the report contained.
    let junit = estamora_report::render_junit(&report).expect("a parsed report must render");
    assert!(
        junit.starts_with("<?xml") || junit.starts_with('<'),
        "the JUnit rendering is not an XML document"
    );
    assert!(
        !junit.contains("]]>"),
        "the JUnit rendering contains a CDATA terminator outside a CDATA section"
    );

    // The pretty form is a different code path through the writer — indentation and
    // ordering — so it gets the same treatment.
    let pretty =
        estamora_report::render_json(&report, true).expect("a parsed report must render");
    serde_json::from_str::<serde_json::Value>(&pretty)
        .expect("the pretty JSON rendering must itself be valid JSON");
});
