//! What it costs to measure a contract.
//!
//! A run is a deployment, a seeding pass, an invocation, a capture of the world before
//! and after, and the evaluation of every requirement the profile declares against the
//! observation. None of that is arithmetic, and the parts that dominate are the ones
//! nobody would guess: constructing a host, registering a contract into it, and
//! evaluating the whole profile for each vector rather than only the requirements that
//! vector names.
//!
//! That last one is deliberate — a vector is measured against every requirement the
//! profile makes, not against the ones it mentions — and it is also the cost that would
//! grow fastest as a profile gains requirements. A benchmark of a single vector is
//! therefore also a benchmark of how a profile's size affects a run.
//!
//! `fixture:none` is the reference contract, so the number here is the cost of a *clean*
//! measurement. A defective contract is a different shape: it aborts, and the assertion
//! evaluator then does more work per failing check rather than less.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a benchmark may panic; a failed run is the signal"
)]
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use estamora_benches as harness;

fn execution(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("run");

    // The control, and the case that exercises every dimension the fixture profile
    // declares on the success path.
    group.bench_function(
        BenchmarkId::new("vector_execution", "conforming"),
        |bencher| {
            let config = harness::run_config("none");
            bencher.iter(|| {
                let outcome = estamora_cli::run(&config).expect("the reference fixture must run");
                std::hint::black_box(outcome.report.status);
            });
        },
    );

    // A refused call: the contract moves value without consulting authorization and
    // never asks for a signature, so the authorization, event, state, invariant and
    // failure dimensions all have something to report. The evaluation is not the same
    // amount of work, which is why it is a separate measurement rather than an
    // assumption.
    group.bench_function(
        BenchmarkId::new("vector_execution", "defective"),
        |bencher| {
            let config = harness::run_config("skips-authorization");
            bencher.iter(|| {
                let outcome =
                    estamora_cli::run(&config).expect("the defective fixture must still run");
                std::hint::black_box(outcome.report.status);
            });
        },
    );

    // A tag filter that selects one vector, so the per-vector cost can be separated from
    // the per-run cost — building the host, resolving the profile, loading the corpus and
    // rendering the report are all per-run and would otherwise be invisible.
    group.bench_function(
        BenchmarkId::new("vector_execution", "one_of_four"),
        |bencher| {
            let mut config = harness::run_config("none");
            config.tags = vec!["balance".to_owned()];
            bencher.iter(|| {
                let outcome = estamora_cli::run(&config).expect("a filtered run must still run");
                std::hint::black_box(outcome.report.status);
            });
        },
    );

    group.finish();

    // Validation without execution, which is what a contributor runs in a loop and what a
    // pull request against a profile triggers. It is a different cost from a run — no host,
    // no contract — and it is the one a person repeats dozens of times an hour.
    criterion.bench_function("validate/bundle_and_corpus", |bencher| {
        bencher.iter(|| {
            let bundle = harness::fixture_bundle();
            let corpus =
                estamora_vectors::VectorCorpus::load(&bundle, harness::fixture_spec_root())
                    .expect("the fixture corpus must load");
            std::hint::black_box(corpus.len());
        });
    });
}

criterion_group!(benches, execution);
criterion_main!(benches);
