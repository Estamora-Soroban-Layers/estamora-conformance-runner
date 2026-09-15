# Fixtures

Everything the test suite measures, refuses or compares against, kept in one place so
that a test names a fixture rather than building one inline.

| Directory | Holds |
| --- | --- |
| [`contracts/`](contracts/) | The fixture contracts, as Rust crates. `conformance-token` is the reference token with exactly one defect injected per variant, so a failure can be traced to one named requirement; `measurable-token` is a SEP-0041 token that exists to be **compiled**, and lives outside the workspace so that it can be. |
| [`wasm/`](wasm/) | The compiled `measurable-token`, committed. `scripts/build-fixture-wasm.sh` produces it and CI rebuilds it and fails if the committed copy differs. |
| [`profiles/`](profiles/) | A complete profile bundle, laid out the way a checkout of `estamora-conformance-spec` is: `profiles/<id>/<version>/`. This is what `--spec fixtures` resolves against. |
| [`malformed-inputs/`](malformed-inputs/) | Documents that must be **refused**, each with the refusal it must produce: a profile missing a document, a vector missing a field, a wrong specification-format version, a broken cross-reference, and a file that is not a report. |
| [`expected-reports/`](expected-reports/) | Stored reports the renderers are compared against, so that a change to a rendering is a visible diff rather than a silently re-baselined snapshot. |

## Why one artifact is committed

The runner has three targets, and the `.wasm` target is the one a user meets first: it is
what `examples/local-contract/` and `docs/local-testing.md` walk through. Measuring it
needs an artifact, and no contract in the runner's workspace can produce one — the
workspace enables the SDK's `testutils` for every crate in it, and `testutils` does not
compile for WebAssembly.

`contracts/measurable-token` therefore declares its own `[workspace]`, and its output is
committed so that the default test suite measures a real compilation without a build step
and without a network. The staleness that arrangement invites — a committed binary whose
source has moved on, still passing every test — is what the CI job that rebuilds it
exists to catch.

## There is deliberately no `vectors/` here

A vector belongs to the profile bundle it is declared against, and the format requires
it to sit inside that bundle:

```text
fixtures/profiles/conformance-token/1.0/
├── profile.yaml
├── methods.yaml
├── behavior.yaml
├── …
└── vectors/
    ├── balance/
    └── transfer/
```

A top-level `fixtures/vectors/` would be a second home for a vector, and the loader
deliberately has no way to read one: a corpus is not a directory listing, it is the set
of vectors the profile *declares* — the operation directories the bundle owns, plus the
shared families under the specification checkout that the bundle says it consumes.
A vector that could be reached by path without a profile declaring it would be a
requirement with no requirement set, which is the one thing this project must not have.

The specification repository keeps its own `vectors/` tree for exactly the reason this
one does not: there, the shared families are declared by many profiles, so they are
stored once beside the profiles rather than copied into each. See
`docs/profile-format.md`.
