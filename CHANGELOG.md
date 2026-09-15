# Changelog

All notable changes to the Estamora conformance runner are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
versioning is [Semantic Versioning](https://semver.org/spec/v2.0.0.html), with one
qualification that matters more here than in most projects.

## How versioning works for this tool

A conformance tool's version is part of what its result means, so three different
versions are in play at once and they move independently:

| Version | Identifies |
| --- | --- |
| The runner version | Which tool reached a verdict. |
| The profile version | Which requirements the verdict is about. |
| `estamora_spec_version` | Which document *format* a profile is written in. |

This repository versions the first. The second and third belong to
[`estamora-conformance-spec`](https://github.com/Estamora-Soroban-Layers/estamora-conformance-spec).

**A change that can change a verdict is a breaking change**, even when no API
changes, because a verdict is the output. Concretely, all of the following are major
for this tool:

* a change to the verdict rule in `estamora-core`;
* a change to how a dimension evaluates a requirement, where the old reading would
  have passed a contract the new reading fails;
* a change to what a report contains, or to the identifier grammar of a check;
* a new limit that can turn an executed vector into a skipped one.

A change to an *error class* or an *exit code* is also major, because a pipeline
gates on it. A new report rendering, a new command, or a new assertion dimension is
minor: it adds output, and a result that did not have it before was not made wrong by
its arrival.

Pre-`1.0`, a patch release may carry a fix that changes behaviour, and it will say so
here. That is the one place the rule above is relaxed, and it is relaxed because an
incorrect verdict is fixed rather than preserved.

## [Unreleased]

## [0.1.0]

### Added

* **The `estamora` CLI**, with six commands: `run`, `inspect`, `profile`, `validate`,
  `report` and `certify`. Each answers a different question, and three of them are
  answerable without executing anything, so *the profile is wrong* can be reported as
  distinct from *the contract is wrong*.
* **Profile consumption.** Bundles are loaded from a specification checkout, validated
  against the published JSON Schemas, and every cross-reference between their six
  documents is resolved. A bundle that is unusable is refused by name rather than
  executed partially.
* **Vector corpus loading and resolution**, including the refusal of a vector that
  refers to a method, event, invariant or failure the profile does not declare.
* **Seven assertion dimensions**, evaluated individually and reported individually:
  interface, authorization, events, behaviour, state, invariants and failure. A
  dimension never collapses its checks into one boolean.
* **Interface inspection** from a contract artifact's `contractspecv0` section, read
  before deployment so that a later abort can be attributed to the contract.
* **Local execution** in a real Soroban host with the ledger sequence, close time and
  every account's authorization fixed by the vector.
* **Report renderings**: the normative JSON document (validated against the
  specification's `report.schema.json`), Markdown for a reviewer, and JUnit for an
  existing CI gate. An undecidable vector reaches JUnit as `error`, never as
  `failure`.
* **Certification receipts**: issue, sign and verify. A receipt commits to a report
  by digest and to the profile and corpus digests, the target and its Wasm hash where
  one was deployed, the status, the exit code and the per-dimension tally.
* **A documented exit-code contract** in which a failure never produces `0`, `1` or
  `2` unless it names the contract, so an unreachable network can never be reported
  as non-conformance.
* **Seven fixture contracts** — the reference token and six copies of it, each wrong
  in exactly one way — compiled from Rust in the workspace, so the end-to-end suite
  needs no network and no artifact to be built by hand.
* **An end-to-end test suite** in `integration-tests/`, one target per dimension,
  driving the library the way a user drives the CLI.
* **Fuzzing targets** for the parsers, and benchmarks for profile loading, vector
  execution, report generation and large-profile handling.
* **Documentation**: `docs/architecture.md`, `docs/execution-engine.md`,
  `docs/cli.md`, `docs/profile-format.md`, `docs/local-testing.md`,
  `docs/testnet-testing.md`, `docs/ci-integration.md`, `docs/certification.md`,
  `docs/security.md` and `docs/troubleshooting.md`.

### Known limitations

* **No network transport.** `--contract <id> --network <name>` is accepted, modelled
  and refused with a named reason (`network-transport-unavailable`), exiting as an
  environment failure rather than producing a verdict. Adding one is adding a
  transport, not a second pipeline.
* **Opening state cannot be established for an arbitrary artifact.** A vector that
  declares an opening balance is `skipped` against a `.wasm` target, with the reason,
  and the run becomes `INCONCLUSIVE`. Establishing it needs a declaration the
  specification format does not yet have — *call this method, with this
  authorization, to reach this state* — and inventing one here would put the runner
  in the position of deciding what a profile may require.
* **Nothing proves security.** Conformance is behavioural compatibility with a named
  profile over a named corpus, and nothing more.

[Unreleased]: https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner/releases/tag/v0.1.0
