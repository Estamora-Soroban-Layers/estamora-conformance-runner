# Contributing

Estamora is two repositories, and the first question about any change is which one
it belongs in.

| If you are changing… | It belongs in |
| --- | --- |
| What conformance *means*: a profile, a requirement, a vector, a schema, a report field | [`estamora-conformance-spec`](https://github.com/Estamora-Soroban-Layers/estamora-conformance-spec) |
| How a requirement is *executed and measured*: resolution, invocation, an assertion evaluator, a report rendering, the CLI | this repository |

This repository contains no normative claims. It does not decide what SEP-41
requires or what a report must contain; it reads those from the specification and
applies them. If a change would make the runner disagree with the specification, the
change is wrong — and if you believe the specification is wrong, the fix is a pull
request there, not an exception here.

## Setting up

```console
$ git clone https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner
$ cd estamora-conformance-runner
$ ./scripts/install.sh          # or: cargo build --workspace
```

Rust is pinned in `rust-toolchain.toml`; `rustup` will fetch the pinned toolchain and
the `wasm32v1-none` target automatically. The workspace targets the 2024 edition and
requires Rust 1.98 or newer.

To run the tests that measure the real SEP-41 profile, check the specification out
beside this repository:

```console
$ git clone https://github.com/Estamora-Soroban-Layers/estamora-conformance-spec ../estamora-conformance-spec
```

Those tests skip themselves when it is absent, so a checkout without it still passes
its whole suite — and still says in the test output that it skipped.

## The workspace

Read `docs/architecture.md` first; it is short and it explains the layering, which is
the one thing you need in order to know where a change goes. The short form:

```
estamora-core          vocabulary: error classes, statuses, the verdict rule, expressions
estamora-profile       loads and validates a profile bundle
estamora-vectors       loads and resolves the corpus
estamora-soroban       the only crate that knows Soroban exists
estamora-assertions    evaluates requirements against an observation (pure)
estamora-report        the report document and its renderings
estamora-certification digests, receipts, signing, verification
estamora-cli           the pipeline and the command surface
```

Two rules are enforced by review and by the layering itself:

* **Nothing below `estamora-soroban` may depend on it.** The specification-consuming
  and reporting layers have to stay usable — and testable — with no execution
  environment.
* **`estamora-assertions` is pure.** It never opens a file, deploys anything or reads
  a clock. It is handed an observation and returns outcomes. A rule that needs a fact
  outside the observation cannot be evaluated, and adding the fact is a change to
  what is observed rather than a lookup inside an evaluator.

### Where a named module lives

If you are looking for one of the module names the project description uses, this is
the correspondence. The architecture was built to the description, but a few modules
were placed where they belong rather than where a file listing would put them, and the
names below are what they are called here.

| Named module | Implemented as | Why |
| --- | --- | --- |
| `estamora-cli/src/errors.rs` | itself | |
| `estamora-cli/src/config.rs`, `output.rs` | themselves | |
| `estamora-cli/src/commands/*.rs` | themselves | |
| `estamora-core/src/engine.rs`, `execution.rs`, `context.rs` | `estamora-cli/src/engine/` (`target.rs`, `scenario.rs`, `observe.rs`, `record.rs`, `values.rs`) | The engine drives a host, so it cannot live in the crate that must stay usable without one. `engine/target.rs` is context and target resolution, `scenario.rs` is one vector's execution, `observe.rs` and `record.rs` capture the two worlds. |
| `estamora-core/src/assertions.rs` | `estamora-assertions` | A whole crate rather than a module, for the same reason. |
| `estamora-core/src/outcomes.rs`, `errors.rs`, `lib.rs` | themselves | |
| `estamora-profile/src/parser.rs` | `documents.rs` | Parsing is per-document and its result is the typed document set. |
| `estamora-profile/src/validator.rs` | `references.rs`, `spec.rs` | Cross-references and format-version checks are two different refusals. |
| `estamora-profile/src/resolver.rs` | `loader.rs` | Resolving a bundle's paths is what loading it means. |
| `estamora-profile/src/types.rs`, `lib.rs` | themselves | |
| `estamora-soroban/src/simulator.rs` | `host.rs` | It is not a simulator: it is a real host at a pinned ledger point. |
| `estamora-soroban/src/interface.rs` | `inspect.rs` | Inspection reads the spec section out of WebAssembly. |
| `estamora-soroban/src/storage.rs` | `world.rs` | Storage is read through `estamora-core`'s `World`, in the vector's vocabulary. |
| `estamora-soroban/src/client.rs`, `invoker.rs`, `events.rs`, `auth.rs`, `errors.rs`, `lib.rs` | themselves | |
| `estamora-vectors/src/resolver.rs` | `loader.rs`, `model.rs` | A vector's operation and its shared families are resolved while loading. |
| `estamora-vectors/src/generator.rs` | *not implemented* | Nothing generates vectors. A vector is a specification artifact authored by a person; a generator would put the runner in the position of proposing what a profile requires. |
| `estamora-vectors/src/loader.rs`, `lib.rs` | themselves, with `integer.rs` and `validate.rs` | |
| `estamora-assertions/src/interface.rs`, `authorization.rs`, `events.rs`, `state.rs`, `failures.rs`, `invariants.rs` | `src/dimensions/*.rs`, same six names | Kept in one directory so that the set of dimensions is visible as a set, and so that `dimensions/mod.rs` is the one place their fixed evaluation order is written. |
| `estamora-assertions/src/lib.rs` | itself, with `eval.rs`, `run.rs`, `observation.rs`, `outcome.rs` | |
| `estamora-report/src/formatter.rs` | `render.rs` for the shared helpers; the format dispatch is `estamora-cli/src/commands/mod.rs` (`Format::render`) | A formatter trait would need three implementations and have one caller. Which rendering to produce is a command-line decision, so it is decided where the command line is read, and what the renderers genuinely share — safe interpolation and appending — is what `render.rs` factors out. |
| `estamora-report/src/summary.rs` | `model.rs` (`Summary`) | The summary is part of the document, not a view of it. |
| `estamora-report/src/model.rs`, `json.rs`, `markdown.rs`, `junit.rs`, `lib.rs` | themselves | |
| `estamora-certification/src/receipt.rs`, `digest.rs`, `signing.rs`, `verification.rs`, `lib.rs` | themselves | |

Renaming a module to match a name in the description is not a change this project
makes on its own: a rename that moves no behaviour is a diff every reviewer has to
read and none can verify, and the table above is the smaller cost. What the table
does not do is excuse a module that *should* exist — the one entry marked *not
implemented* is there because nothing needs it, not because it was skipped.

## Building and testing

```console
$ cargo build --workspace
$ cargo test --workspace
$ cargo fmt --all
$ cargo clippy --workspace --all-targets -- -D warnings
```

The workspace lints are strict on purpose and they apply to every crate:
`unwrap_used` and `expect_used` are denied, `print_stdout` and `print_stderr` are
denied, and an `#[allow(...)]` without a `reason` is an error. In a tool that parses
untrusted documents and prints structured output, a silent panic and a stray
`println!` are both real defects, and a suppression without a reason is how a lint
quietly stops applying to the code that most needs it.

Everything in one command:

```console
$ ./scripts/run-ci.sh
```

It is the same sequence CI runs, so a red job can be reproduced locally.

## Adding an assertion type

1. Implement the evaluator in `estamora-assertions/src/dimensions/`. It receives the
   observation and returns `AssertionOutcome`s, one per check — a dimension never
   collapses several checks into one boolean.
2. Give every outcome an **identifier** that is stable and structured
   (`dimension/kind/subject/…`) and a `detail` that says what the check was. The
   identifier is what a report and a receipt commit to; the detail is what a reader
   searches.
3. Register it in the run loop. If the check cannot be made, record an
   `AssertionOutcome` that is neither passed nor failed and attaches a diagnostic
   naming the reason. **Never record a pass for a check that was not made** — that is
   the one thing this project cannot tolerate, because a fabricated pass is how a
   suite stops meaning anything.
4. Add a test to `estamora-assertions/src/tests.rs` and, if it changes what a run
   reports end to end, one to `integration-tests/`.

## Adding profile support

Usually nothing: a profile is a document bundle, and `docs/profile-format.md`
describes what the loader accepts. If you are adding a *format capability* — a new
kind of expression, a new binding — that is a change to the specification's schemas
first and to this runner second, and the specification change is the one to argue for.

## Adding a report format

Implement it in `estamora-report` against the report model. The model is the
specification's `report.schema.json` shape, so a new rendering cannot invent a field
— and `estamora-report/tests/spec_schema.rs` validates the JSON rendering against
that schema, so a format that disagrees with it fails the build.

## Adding vectors

Vectors belong to a profile and live in the specification repository. Locally, the
corpus this repository measures its own fixtures with is
`fixtures/profiles/conformance-token/1.0/vectors/`, and adding one there is a good way
to develop a new assertion: a vector is the only thing that makes a rule measurable.

```console
$ cargo run -p estamora-cli -- validate --spec fixtures --profile conformance-token@1.0
$ cargo run -p estamora-cli -- run --spec fixtures --profile conformance-token@1.0 --contract fixture:none
```

## Fuzzing and benchmarks

```console
$ cargo fuzz list                 # in fuzz/, requires cargo-fuzz
$ cargo bench --workspace
```

Fuzzing targets the parsers — a profile, a vector, an assertion expression, a report —
because those are the boundaries where untrusted input arrives. Benchmarks exist to
notice a regression in profile loading, vector execution, report generation and
large-corpus handling, not to produce numbers for a README.

Neither proves anything about a contract's security. See `docs/security.md`.

## What a change has to satisfy

* **No fabricated passes.** If a check could not be made, say so.
* **No invented requirements.** The runner has no authority to decide what a standard
  requires.
* **A verdict is derived, never asserted.** The rule is in `estamora-core`; a change
  to it is a change to what every report means.
* **Errors keep their class.** A new failure names one of the existing classes, and
  an unreachable network never becomes non-conformance.
* **The exit code contract holds.** It is documented in `docs/cli.md`, and CI relies
  on it.

## Pull requests

* One concern per pull request. A behavioural change and a refactor in one diff
  cannot be reviewed together, and the second will hide the first.
* The description should say **why**, and what would have to be true for the change
  to be wrong. A commit that fixes a real defect is worth more than one that adds an
  API.
* If you changed what a run reports, say which test changed and why the new output is
  correct.
* If you found a defect while writing a feature, a separate pull request for the fix
  is usually the right split — it can be merged and released without waiting for the
  feature.

`./scripts/run-ci.sh` must pass. A red job is not a request for a reviewer to guess.

## Releasing

```console
$ ./scripts/release.sh <version>
```

checks the changelog, the versions and the workspace before anything is published. It
does not push or tag; the tag is a human step, because a conformance tool's release
is part of what a result means. `CHANGELOG.md` explains the versioning policy.
