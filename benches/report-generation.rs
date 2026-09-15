//! What it costs to render a result.
//!
//! Three renderings of one model, and they are not interchangeable in cost. The JSON
//! document is a serialisation of the report as it stands. Markdown is a walk that
//! formats every check. `JUnit` is a walk that also has to escape text into a container
//! format, which is the one that does real work per check — and the one whose cost would
//! grow with a report that got larger, which is what a real profile produces.
//!
//! Both the reading and the rendering are measured, because a stored report is re-read
//! and re-rendered rather than re-run: `estamora report` exists so that a reviewer can
//! look at a result months later without measuring anything again. A regression in the
//! parser would show up as a slow review, and nobody would connect the two.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a benchmark may panic; a failed run is the signal"
)]
use criterion::{Criterion, criterion_group, criterion_main};

use estamora_benches as harness;

fn rendering(criterion: &mut Criterion) {
    let text = harness::stored_report_text();
    let report = harness::stored_report();

    criterion.bench_function("report/parse", |bencher| {
        bencher.iter(|| {
            let parsed = estamora_report::parse(std::hint::black_box(&text))
                .expect("the stored report must parse");
            std::hint::black_box(parsed.status);
        });
    });

    criterion.bench_function("report/render_json", |bencher| {
        bencher.iter(|| {
            let document = estamora_report::render_json(std::hint::black_box(&report), false)
                .expect("a parsed report must render");
            std::hint::black_box(document.len());
        });
    });

    criterion.bench_function("report/render_markdown", |bencher| {
        bencher.iter(|| {
            let document = estamora_report::render_markdown(std::hint::black_box(&report))
                .expect("a parsed report must render");
            std::hint::black_box(document.len());
        });
    });

    criterion.bench_function("report/render_junit", |bencher| {
        bencher.iter(|| {
            let document = estamora_report::render_junit(std::hint::black_box(&report))
                .expect("a parsed report must render");
            std::hint::black_box(document.len());
        });
    });

    // A receipt commits to a report rather than rendering it, and the digest is the only
    // unbounded work in it: hashing whatever the document happens to be. Cheap, and worth
    // pinning, because a receipt is issued on every run that asks for one.
    criterion.bench_function("report/digest", |bencher| {
        let json = estamora_report::render_json(&report, false).expect("a report must render");
        bencher.iter(|| {
            let digest =
                estamora_certification::Digest::of_bytes(std::hint::black_box(json.as_bytes()));
            std::hint::black_box(digest);
        });
    });
}

criterion_group!(benches, rendering);
criterion_main!(benches);
