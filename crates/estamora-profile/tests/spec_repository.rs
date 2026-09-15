//! The cross-repository check.
//!
//! The unit tests load bundles this crate writes, which proves the loader is
//! self-consistent and proves nothing about whether the typed model matches the
//! documents the specification repository actually publishes. This test closes
//! that gap: it loads the real bundles from a checkout of the specification
//! repository and asserts what they declare.
//!
//! It is marked `#[ignore]` rather than skipped, because "not run" and "passed"
//! must not look the same. Run it with:
//!
//! ```text
//! ESTAMORA_SPEC_REPO=/path/to/estamora-conformance-spec \
//!   cargo test -p estamora-profile -- --ignored
//! ```
//!
//! When the variable is unset the test fails rather than passing quietly: it was
//! asked to run and could not, and that is a result the caller needs to see.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests may panic; a failing test is the signal"
)]

use std::path::PathBuf;

use estamora_profile::{ProfileBundle, RequirementStatus};

fn spec_repository() -> PathBuf {
    std::env::var("ESTAMORA_SPEC_REPO").map_or_else(
        |_: std::env::VarError| {
            panic!(
                "ESTAMORA_SPEC_REPO must name a checkout of estamora-conformance-spec; \
                 this test is ignored by default because it needs a second repository"
            )
        },
        PathBuf::from,
    )
}

#[test]
#[ignore = "needs a checkout of estamora-conformance-spec; set ESTAMORA_SPEC_REPO and pass --ignored"]
fn the_sep_41_bundle_loads_and_declares_the_whole_token_interface() {
    let root = spec_repository().join("profiles/sep-41/1.0");
    let bundle = ProfileBundle::load(&root).unwrap_or_else(|failure| {
        panic!(
            "the released SEP-41 bundle must load: {} ({:?})",
            failure.message(),
            failure.blame()
        )
    });

    assert_eq!(bundle.reference().to_string(), "sep-41@1.0");
    assert_eq!(
        bundle.document().profile.specification.name,
        "SEP-0041",
        "the profile must name the upstream document it encodes"
    );

    let methods = bundle.methods();
    let mut names: Vec<&str> = methods
        .methods
        .iter()
        .map(|method| method.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "allowance",
            "approve",
            "balance",
            "burn",
            "burn_from",
            "decimals",
            "name",
            "symbol",
            "transfer",
            "transfer_from",
        ],
        "the profile claims full interface coverage, so every method of the \
         upstream trait must be modelled"
    );

    // Every method the profile claims as required, it must also be able to say
    // what it requires. A method with no behaviours, no failures and no events
    // would be a method the profile checks only for existence.
    for method in &methods.methods {
        assert_eq!(method.requirement, RequirementStatus::Required);
        assert!(
            !method.summary.is_empty(),
            "{} must state its requirement",
            method.name
        );
        assert!(
            !method.behaviors.is_empty() || !method.failures.is_empty(),
            "{} is required but constrains no behaviour and no failure, so the \
             profile would only check that it exists",
            method.name
        );
    }
}

#[test]
#[ignore = "needs a checkout of estamora-conformance-spec; set ESTAMORA_SPEC_REPO and pass --ignored"]
fn every_published_bundle_loads() {
    let profiles = spec_repository().join("profiles");
    let mut loaded = Vec::new();

    for entry in std::fs::read_dir(&profiles).expect("profiles/ must be readable") {
        let entry = entry.expect("a directory entry");
        if !entry.path().is_dir() {
            continue;
        }
        // A bundle may sit at profiles/<id>/<version>/ or directly at
        // profiles/examples/<id>/. Both are loaded when they declare an entry
        // point, and the loader itself decides whether the layout is canonical.
        let mut candidates = vec![entry.path()];
        if let Ok(children) = std::fs::read_dir(entry.path()) {
            candidates.extend(
                children
                    .filter_map(Result::ok)
                    .map(|child| child.path())
                    .filter(|path| path.is_dir()),
            );
        }
        for candidate in candidates {
            if !candidate.join("profile.yaml").is_file() {
                continue;
            }
            let bundle = ProfileBundle::load(&candidate).unwrap_or_else(|failure| {
                panic!(
                    "{} must load: {} ({:?})",
                    candidate.display(),
                    failure.message(),
                    failure.blame()
                )
            });
            loaded.push(bundle.reference().to_string());
        }
    }

    loaded.sort();
    assert!(
        loaded.iter().any(|reference| reference == "sep-41@1.0"),
        "the released profile must be among the bundles found: {loaded:?}"
    );
    assert!(
        loaded.len() > 1,
        "the worked examples must load too, since they are validated by the same \
         tooling as the released profile: {loaded:?}"
    );
}
