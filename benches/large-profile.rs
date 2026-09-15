//! How loading scales with the size of a bundle.
//!
//! The fixture bundle is four methods and four vectors, which is a fine thing to
//! validate in a loop and a useless thing to measure scaling with. Real profiles are
//! bigger — SEP-41 declares twelve methods and a corpus that consumes two shared vector
//! families — and the interesting question is not "how long does four take" but "what
//! happens at four hundred".
//!
//! The specific regression this exists to catch is a resolution step that is quadratic
//! without anybody noticing. Checking every rule's `methods` list against every declared
//! method is O(methods × rules), constructing a path per reference is O(references)
//! allocations, and both look fine at four and terrible at four hundred. A benchmark that
//! scales the input is the only way that shows up before a contributor does.
//!
//! What is measured is the *loader*, and only the loader: the generated bundles are valid
//! and the corpus resolves, so no time here is spent in an error path.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a benchmark may panic; a failed run is the signal"
)]
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use estamora_benches as harness;

/// The sizes measured, chosen so that a single one of them being out of line is visible
/// rather than being averaged away. Four is the fixture set's own size, and it is included
/// so that a number taken from here can be compared with one taken from
/// `profile-loading.rs`.
const SIZES: &[(usize, usize)] = &[(4, 4), (50, 50), (200, 200)];

fn large(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("large_profile");

    // A generated bundle is written once per size and then loaded many times: the
    // measurement is of loading, and writing the files is setup.
    for &(methods, vectors) in SIZES {
        let directory = harness::generate_bundle(methods, vectors);
        let root = directory.path().join("bundle");

        group.bench_with_input(
            BenchmarkId::new("load_bundle", format!("{methods}m-{vectors}v")),
            &root,
            |bencher, root| {
                bencher.iter(|| {
                    let bundle = estamora_profile::ProfileBundle::load(root)
                        .expect("a generated bundle must load");
                    std::hint::black_box(bundle.document());
                });
            },
        );

        // Loading the corpus as well, which is the step that walks a directory tree and
        // checks every vector against the profile it was found under. This is the half of
        // the work that grows with the corpus rather than with the interface.
        group.bench_with_input(
            BenchmarkId::new("load_bundle_and_corpus", format!("{methods}m-{vectors}v")),
            &root,
            |bencher, root| {
                let spec = root
                    .parent()
                    .expect("the bundle has a parent")
                    .to_path_buf();
                bencher.iter(|| {
                    let bundle = estamora_profile::ProfileBundle::load(root)
                        .expect("a generated bundle must load");
                    // The specification root, not the bundle root: the loader resolves
                    // the profile's own vector directories beneath the bundle and the
                    // shared families beneath the specification tree. This bundle
                    // declares no shared families, so the tree it is given makes no
                    // difference to what is loaded — only to whether the path arithmetic
                    // runs.
                    let corpus = estamora_vectors::VectorCorpus::load(&bundle, &spec)
                        .expect("a generated corpus must load");
                    std::hint::black_box(corpus.len());
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, large);
criterion_main!(benches);
