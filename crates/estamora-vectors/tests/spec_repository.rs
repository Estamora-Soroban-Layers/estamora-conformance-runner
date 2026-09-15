//! Loading the corpus the specification repository actually publishes.
//!
//! This is the test that keeps the two repositories honest with each other. Every
//! cross-reference rule the loader enforces is a claim about *their* documents, and
//! a claim about someone else's documents is only worth anything if it is checked
//! against them rather than against a fixture that was written to satisfy it.
//!
//! It needs a checkout of `estamora-conformance-spec`, so it is ignored by default
//! and runs from CI, where both repositories are present.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]

use std::path::PathBuf;

use estamora_profile::ProfileBundle;
use estamora_vectors::VectorCorpus;

/// The checkout of the specification repository, when there is one.
fn spec_root() -> Option<PathBuf> {
    let root = std::env::var_os("ESTAMORA_SPEC_REPO")?;
    let root = PathBuf::from(root);
    root.is_dir().then_some(root)
}

#[test]
#[ignore = "needs a checkout of estamora-conformance-spec; set ESTAMORA_SPEC_REPO and pass --ignored"]
fn the_sep_41_corpus_loads_against_the_sep_41_profile() {
    let Some(root) = spec_root() else {
        return;
    };
    let bundle = ProfileBundle::load(root.join("profiles/sep-41/1.0")).unwrap();
    let corpus = VectorCorpus::load(&bundle, root.join("vectors")).unwrap();

    // Twelve vectors the profile owns across its ten operation directories, plus
    // the eight the shared library contributes through the two families it
    // declares. Both routes are asserted separately below, because the total alone
    // would not notice one of them failing to load.
    assert!(
        corpus.len() >= 20,
        "the SEP-41 corpus is expected to hold at least the twenty vectors the \
         repository publishes; found {}",
        corpus.len()
    );

    // The profile owns vectors for ten operation directories, and consumes two
    // shared families. Both routes have to be represented, or the loader would be
    // passing while only reading half of what a bundle declares.
    let owned = corpus
        .vectors()
        .iter()
        .filter(|vector| !vector.is_profile_independent())
        .count();
    let shared = corpus
        .vectors()
        .iter()
        .filter(|vector| vector.is_profile_independent())
        .count();
    assert!(owned > 0, "the profile's own vectors must be loaded");
    assert!(shared > 0, "the shared vectors must be loaded");

    // A corpus of only positive vectors cannot establish behavioural conformance,
    // so the specification requires the other kinds and the loader has to carry
    // them through rather than treating them as decoration.
    for id in [
        "transfer-moves-exact-amount",
        "transfer-without-signature-fails",
        "transfer-from-wrong-actor-fails",
        "unknown-account-balance-is-zero",
    ] {
        assert!(corpus.by_id(id).is_some(), "the corpus must include {id:?}");
    }

    assert!(
        corpus.skipped().is_empty(),
        "no published vector declares a method the profile does not implement, so none \
         should have been excluded: {:?}",
        corpus.skipped()
    );
    assert!(
        corpus.diagnostics().is_empty(),
        "a published corpus must resolve completely, found {:?}",
        corpus.diagnostics()
    );
}

#[test]
#[ignore = "needs a checkout of estamora-conformance-spec; set ESTAMORA_SPEC_REPO and pass --ignored"]
fn every_published_vector_selects_by_its_own_tags() {
    let Some(root) = spec_root() else {
        return;
    };
    let bundle = ProfileBundle::load(root.join("profiles/sep-41/1.0")).unwrap();
    let corpus = VectorCorpus::load(&bundle, root.join("vectors")).unwrap();

    // Tag selection is what CI uses to run a subset, so every vector has to be
    // reachable through the tags it declares. A vector that selected itself out
    // would be one no CI job ever ran.
    for vector in corpus.vectors() {
        let tags: Vec<String> = vector.document().tags.clone();
        assert!(!tags.is_empty(), "{:?} declares no tags", vector.id());
        if let Some(first) = tags.first() {
            let selected = corpus.matching(std::slice::from_ref(first));
            assert!(
                selected.iter().any(|found| found.id() == vector.id()),
                "{:?} is not reachable through its own tag {first:?}",
                vector.id()
            );
        }
    }
}
