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

### Changed

* **Every entry point builds each storage key once.** `move_value`, `burn`, `burn_from` and
  `transfer_from` each named the entry they were about to read and then named it again to
  write it, and a `DataKey` carries an `Address` that is encoded before it reaches the
  storage layer. The key is now built once and reused. No behaviour changes: the same entries
  are read and written, the same number of times, in the same order, and every refusal is
  reached by the same path.

  The saving is in the compiled contract rather than in the resource fee, and that
  distinction is measured rather than assumed. Costing every entry point in the local host
  before and after gives **identical instruction counts**; the artifact is **115 bytes
  smaller**, 11,324 to 11,209 (1.0%). What a *call* pays for is the storage reads and writes
  — two reads and two writes for a transfer, which is the arithmetic the operation requires —
  and what a *deployment* pays for is the size of the code that performs them.

  The example token was **redeployed** from this source, so the capture and the deployment name
  the same bytes again. `examples/testnet-contract/costs.json` and `report.json` were both
  re-measured against the new deployment, and the report's `target.wasm_hash` is
  `5cbcc4ab…9f8707cc` — the digest of the artifact this tree commits, which is what
  `the_committed_network_measurement_reads_re_renders_and_agrees_with_the_artifact` asserts and
  what a report is worth nothing without.

  The re-measurement is also the second reading of the claim above, taken through the network
  rather than in the local host. Per-call figures moved by a few dozen instructions in either
  direction and the resource fees by a handful of stroops — ledger state, not code — while the
  artifact that is deployed is 115 bytes smaller than the one it replaced. A call costs what its
  reads and writes cost; a deployment costs the size of the code that performs them.

  `examples/testnet-contract/README.md` records all three deployments: the two superseded ones by
  identifier and digest, and the current pairing.

## [0.1.3] - 2026-09-15

Every binary released before this one was unable to run a measurement against a `fixture:`
target — the documented way to exercise the runner without a deployed contract, and the
command in the README's own quickstart. It exited `134` with nothing at all on stdout or
stderr. Measurement from a compiled artifact (`--contract <a .wasm>`) was unaffected, so a
released binary could measure a deployed contract and could not measure the reference one.

If you pinned `0.1.2` or earlier, the archives on those releases are affected. The
installer's `--version` flag pins a release, and this is the release to pin.

### Fixed

* **A released binary can produce a verdict.** `[profile.release]` set `panic = "abort"`.
  A Soroban contract reports a failure by panicking, and the `fixture:` path goes through
  `soroban-env-host`'s test utilities, which replace the panic hook so that a panic from
  inside a contract prints nothing, and then catch the unwind to turn it into a host error.
  Under `abort` there is no unwind to catch and no hook left to print with, so the first
  vector that expects a failure ended the process — and the corpus contains negative vectors
  by design, so this was the first thing to happen in any run:

  ```
  $ estamora run --spec fixtures --profile conformance-token@1.0 --contract fixture:none
  Aborted (core dumped)          # exit 134, zero bytes of stdout and stderr
  ```

  The same tree built for debug exited `0` and `1` exactly as documented, which is the
  whole reason every test in this repository passed. `panic` is now stated as `unwind`
  rather than left implicit, with the reason beside it, because implicit is how it came to
  be `abort`: nothing above the profile mentioned a panic strategy at all.

### Changed

* **The exit-code contract is asserted on the profile that ships.** CI's `exit-code contract`
  job built a debug binary, so the contract it verified was the debug profile's contract, and
  the profile is where the behaviour differed. It now builds `--release` and asserts all six
  documented codes — `0`, `1`, `3`, `5`, `64` and `4` — including a conformant run exiting
  `0`, which the job had been missing. The measurement is printed rather than discarded,
  because the failure this now catches printed nothing.
* **The release workflow runs the binary it is about to attach.** `--version` was the only
  thing that executed a built binary before it was packed, and `--version` exercises argument
  parsing and nothing else — every binary this project released before this change passed it
  and then aborted on a real measurement. Each platform now measures the reference fixture
  with the exact file that is about to be packed, so a binary that cannot produce a verdict
  cannot be attached to a release.

## [0.1.2] - 2026-09-15

The release that prepared the crates for crates.io, and the two report defects that would
have been permanent once a version had been published. A version on crates.io cannot be
withdrawn, so the choice was a release that carries the fixes or a release that does not.

A patch, under the qualification at the top of this file rather than in spite of it. Both
fixes change report *content*, which the policy there calls a major change, and the
relaxation for a pre-`1.0` patch is what applies: these are wrong values being corrected,
not a format being redesigned. There is also nobody to break — this is the first
publication — and a report whose `$schema` names a host that does not resolve is not a
format worth preserving for compatibility's sake.

### Added

* **`scripts/check-publish-plan.py`**, which reads the crates.io publish sequence out of
  `.github/workflows/release.yml` and checks it against what Cargo resolves: the sequence
  has to be exactly the publishable crates whose whole normal-dependency closure is itself
  publishable, in an order Cargo can follow. Every exclusion has to name a structural
  reason, so an exclusion that outlives its cause is reported instead of quietly keeping a
  command unobtainable. `scripts/test-check-publish-plan.sh` runs it against mutated copies
  of the real workflow, one per way the plan can be wrong, and CI has a `release plan` job
  that runs both.

### Fixed

* **A report names a schema that exists.** Every report's `$schema` pointed at
  `https://estamora.dev/schema/report.schema.json`, a host with no DNS record, so a consumer
  who followed it to validate a report against the schema the report claims to conform to
  had nothing to fetch. It now names the document the specification serves, and a test
  asserts the constant against that document's own `$id`, so the two cannot drift apart
  again.
* **A code hash has one form, whichever route read the bytes.** A report of a `.wasm`
  artifact recorded `sha256:35d0ef…`, while the same contract measured over RPC recorded
  bare hex. `report.schema.json` requires `^[0-9a-f]{64}$`, so a report produced from a
  local artifact did not validate against the schema it names, and one deployment had two
  spellings depending on how it was read. Both routes now record the 64-character lowercase
  form, and the test that checks the report against the schema carries a real hash, because
  the report it used to build had none and the field's format was never validated.
* **The documented publish sequence no longer names a command that cannot finish.** It ended
  with `cargo publish -p estamora-cli`, and that command can never succeed: the CLI takes
  `estamora-fixture-token` as a normal dependency, the fixture is `publish = false`, and
  packaging rewrites a path dependency into a registry requirement of the same version — so
  the requirement the command would carry names a version no registry will hold. The command
  is distributed as a release binary and through `cargo install --git`; the README and the
  workflow now say which crates go to crates.io and which do not.

## [0.1.1] - 2026-09-15

This release makes the tool installable. No verdict changes: the rule in `estamora-core`
that reduces results to a verdict, every dimension's evaluation, and the report format are
untouched, so a contract measured against the same profile reaches the same result under
`0.1.0` and `0.1.1`.

### Added

* **A prebuilt binary for every supported platform**, attached to each release together
  with a `SHA256SUMS` file. `estamora` previously had no install path at all — no crate on
  crates.io, no package on npm, and a release that carried no files — so the only way to
  obtain the tool was to clone this repository and compile a Soroban host, a TLS stack and
  their entire transitive tree. Each archive carries its own README, its licence and a
  `VERSION` file written by running `--version`.
* **`scripts/install-binary.sh`**, which fetches the archive for the detected platform and
  verifies it against the release's published checksums before installing anything. A
  release with no checksum file, a checksum file with no entry for the archive, and a
  mismatch against the published digest all stop the install rather than continuing with a
  warning. The one place in this project where a network answer decides what gets executed
  is not going to be the one place that trusts it.
* **`scripts/test-install-binary.sh`**, which builds a release in a temporary directory,
  serves it over `file://` and asserts four refusals and two working installs, including
  the documented quickstart. Each refusal case also asserts that nothing was written, since
  an installer that fails and leaves a file behind has not failed.
* **An `installer` job in CI**, which runs that test and lints every shell script in
  `scripts/` with the runner's preinstalled `shellcheck`.
* **An installation section in the README**, in three routes, each stated in terms of what
  it says about which revision is running.
* **A code of conduct**, with a section specific to a tool whose output is a verdict about
  somebody else's contract.
* **`readme` metadata on all eight published crates**, and `scripts/check-crate-metadata.py`,
  which reads what Cargo resolves rather than what the manifests say.

### Changed

* **The Action can be referenced.** `action.yml` existed and was well built, but no
  major-version tag did, so `uses: …/estamora-conformance-runner@v1` resolved to nothing at
  all. `v1` now exists, and `docs/ci-integration.md` documents it in place of the
  clone-and-build integration it described while there was no alternative.
* **The release workflow triggers on three version components**, so a `v1` action tag no
  longer starts a release that fails its own tag-versus-manifest check.

### Fixed

* **The installer works when piped to `sh`.** It ran under `set -o pipefail`, which is not
  in POSIX, and on Debian, Ubuntu and the hosted runners `/bin/sh` is dash: the documented
  `curl -fsSL <url> | sh` stopped on its first line, installed nothing, and reported the
  error only on stderr inside a pipeline where the exit code is the last command's. The
  script is now POSIX `sh` under a `#!/bin/sh` shebang, and the quickstart is a case in the
  installer test rather than a promise in a README.
* **The missing-specification-checkout error names the clone command.** A binary installed
  from a release archive is the sibling of nothing, so the default checkout path can never
  be satisfied by having installed the tool — which is exactly when that message is
  reached, and it previously said only that a directory was missing.
* **The `[Unreleased]` section described work that had already shipped.** The instance-entry
  measurement, `LocalHost::holding`, the compiled fixture contract and the `target::deploy`
  change were all contained in `v0.1.0`; their entries belong under `0.1.0` and have been
  moved there. A changelog that reports released work as pending is worse than one that
  omits it, because a reader checking what a version contains is told the wrong thing.

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
* **Eight fixture contracts** — the reference token and seven copies of it, each wrong
  in exactly one way — written against the SDK in the workspace and registered from
  their Rust types, so the end-to-end suite needs no network and no artifact to be
  built by hand.
* **An end-to-end test suite** in `integration-tests/`, one target per dimension,
  driving the library the way a user drives the CLI.
* **Fuzzing targets** for the parsers, and benchmarks for profile loading, vector
  execution, report generation and large-profile handling.
* **Documentation**: `docs/architecture.md`, `docs/execution-engine.md`,
  `docs/cli.md`, `docs/profile-format.md`, `docs/local-testing.md`,
  `docs/testnet-testing.md`, `docs/ci-integration.md`, `docs/certification.md`,
  `docs/security.md` and `docs/troubleshooting.md`.
* **A deployed contract is measured from its own instance ledger entry** rather than
  redeployed. Resolving a contract already reads that entry, and it carries the instance
  storage the constructor wrote, so the code and the entry are placed in a ledger built
  from the network's answer and the host is built from it. Nothing runs the constructor,
  and the contract is reachable at the identifier the network serves it at. A contract
  whose constructor takes arguments is consequently measurable.
* **`LocalHost::holding`**, which assembles a host around an artifact and the instance
  entry that names it, refusing a pair that does not agree.
* **A contract that is compiled to WebAssembly**, so the `.wasm` target can be measured
  by the test suite. `fixtures/contracts/measurable-token` declares its own workspace —
  the runner's enables the SDK's `testutils` for every member, and that does not compile
  for WebAssembly — and `scripts/build-fixture-wasm.sh` produces the artifact committed
  under `fixtures/wasm/`. CI rebuilds it and fails if the committed copy differs.

### Changed

* **`target::deploy` returns the host along with the deployment.** A remote target is not
  deployed *into* a host, it *is* the host, so the caller has to measure in the ledger the
  contract was placed in.

### Removed

* **`reason: constructor-needs-arguments` no longer applies to a deployed contract.** It
  remains the outcome for a `.wasm` file whose constructor declares arguments, which is
  the path that has no instance entry to place in the ledger.

### Known limitations

* **A `.wasm` file whose constructor takes arguments cannot be registered.**
  Instantiating a local artifact runs its `__constructor`, and no environment supplies
  the arguments a deployed contract's constructor took — only its deployer knew them. The
  runner refuses such an artifact with `reason: constructor-needs-arguments` and exits
  `4`, rather than inventing arguments and fabricating the state the vectors are then
  measured against. A contract with no constructor, or one that takes none, is measured
  normally, and a **deployed** contract is measured from its own instance ledger entry
  regardless of what its constructor took.
* **Opening state cannot be established for an arbitrary artifact.** A vector that
  declares an opening balance is `skipped` against a `.wasm` target, with the reason,
  and the run becomes `INCONCLUSIVE`. Establishing it needs a declaration the
  specification format does not yet have — *call this method, with this
  authorization, to reach this state* — and inventing one here would put the runner
  in the position of deciding what a profile may require.
* **Nothing proves security.** Conformance is behavioural compatibility with a named
  profile over a named corpus, and nothing more.

[Unreleased]: https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner/releases/tag/v0.1.3
[0.1.2]: https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner/releases/tag/v0.1.2
[0.1.1]: https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner/releases/tag/v0.1.1
[0.1.0]: https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner/releases/tag/v0.1.0
