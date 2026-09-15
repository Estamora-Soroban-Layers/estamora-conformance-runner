//! What it costs to load a profile bundle.
//!
//! This is the cost a contributor pays on every `estamora validate`, on every `estamora
//! profile`, and at the start of every run, so it is the one number in this workspace
//! that a person actually waits on. It is also the number that would quietly get worse:
//! adding a check to the cross-reference validator is a change nobody would notice until
//! a bundle with a few hundred requirements took a minute to load.
//!
//! Two things are measured separately, because they have different shapes. Reading the
//! six documents is proportional to their size, and resolving the references between
//! them is closer to quadratic if it is done naively — every rule's `methods` list
//! checked against every method — which is exactly the kind of regression a number is
//! for.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a benchmark may panic; a failed run is the signal"
)]
use criterion::{Criterion, criterion_group, criterion_main};

use estamora_benches as harness;

fn loading(criterion: &mut Criterion) {
    let root = harness::fixture_profile_root();
    let spec = harness::fixture_spec_root();

    criterion.bench_function("profile/load_bundle", |bencher| {
        bencher.iter(|| {
            let bundle =
                estamora_profile::ProfileBundle::load(&root).expect("the fixture bundle must load");
            // The loader's own checks run during `load`, but reading the documents back
            // is part of what a caller does with the result, and a bundle that stopped
            // producing documents would otherwise be measured as free.
            std::hint::black_box(bundle.document());
        });
    });

    criterion.bench_function("profile/load_bundle_and_corpus", |bencher| {
        bencher.iter(|| {
            let bundle =
                estamora_profile::ProfileBundle::load(&root).expect("the fixture bundle must load");
            let corpus = estamora_vectors::VectorCorpus::load(&bundle, &spec)
                .expect("the fixture corpus must load");
            std::hint::black_box(corpus.len());
        });
    });

    // The reference parser, on its own, because it is the one entry point in the loader
    // that a document reaches directly — `--profile id@version` — and because it is
    // cheap enough that a per-call allocation would show.
    criterion.bench_function("profile/parse_reference", |bencher| {
        bencher.iter(|| {
            let reference = estamora_profile::ProfileReference::parse("sep-41@1.0")
                .expect("a well-formed reference must parse");
            std::hint::black_box(reference);
        });
    });
}

criterion_group!(benches, loading);
criterion_main!(benches);
