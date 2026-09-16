# estamora-conformance-runner

[![CI](https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner/actions/workflows/ci.yml/badge.svg)](https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner/actions/workflows/ci.yml)
[![Integration](https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner/actions/workflows/integration.yml/badge.svg)](https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner/actions/workflows/integration.yml)
[![Coverage](https://img.shields.io/badge/line%20coverage-%E2%89%A5%2070%25%20enforced-brightgreen)](#test-coverage)
[![Documentation](https://img.shields.io/badge/docs-estamora--docs.vercel.app-blue)](https://estamora-docs.vercel.app)
[![Product pitch](https://img.shields.io/badge/watch-5--minute%20pitch-blueviolet)](https://estamora-docs.vercel.app/assets/estamora-pitch.mp4)
[![Measured on testnet](https://img.shields.io/badge/measured%20on-testnet-steelblue)](examples/testnet-contract/README.md)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**[Watch the five-minute product pitch](https://estamora-docs.vercel.app/assets/estamora-pitch.mp4)**
— it includes a real run of this binary, captured from the release build rather than staged. It is
served by the documentation site so the browser plays it rather than downloading it; the
[release copy](https://github.com/Estamora-Soroban-Layers/estamora-docs/releases/download/pitch-v1/estamora-pitch.mp4)
is the archived download.

Estamora answers one question about a Soroban contract:

> Does this deployed contract actually behave according to the standard or profile it claims
> to implement?

Exposing the expected methods is not the answer. A contract can implement `transfer`,
`approve` and `balance` with exactly the right signatures and still move the wrong amount,
credit the wrong account, skip the authorization check, emit no event, or return an error
after mutating state. This repository executes a contract against the requirements defined
in the specification repository and reports, per assertion, whether it conformed.

## Where this sits

Estamora is four repositories. The boundary between the first two is the design: one defines
conformance, the other measures it, and neither depends on the other's implementation.

| Repository | Owns |
| --- | --- |
| [`estamora-conformance-spec`](https://github.com/Estamora-Soroban-Layers/estamora-conformance-spec) — [read it](https://estamora-soroban-layers.github.io/estamora-conformance-spec/) | What conformance *means*: profiles, vectors, schemas and the validation tooling |
| `estamora-conformance-runner` (this repository) | Measuring a contract against those requirements and reporting the result |
| [`estamora-docs`](https://github.com/Estamora-Soroban-Layers/estamora-docs) — [read it](https://estamora-docs.vercel.app) | Explaining both of the above: guides, and this repository's own `docs/` assembled at a pinned revision |
| [`estamora-app`](https://github.com/Estamora-Soroban-Layers/estamora-app) | Presenting conformance evidence for live testnet contracts. It reads results; it cannot produce a verdict |

The specification repository is normative. This repository contains no requirements of its
own: it loads a profile, executes the vectors that belong to it, and reports what happened.
Where this runner and the specification could disagree, the specification is right and this
repository has a defect.

## Documentation

| Document | Answers |
| --- | --- |
| [`docs/architecture.md`](docs/architecture.md) | How the layers divide, how the crates are layered, and where a failure is attributed. |
| [`docs/execution-engine.md`](docs/execution-engine.md) | What one vector's execution observes, how each of the seven dimensions is evaluated, and how checks become a verdict. |
| [`docs/cli.md`](docs/cli.md) | Every command, flag, format and exit code, and why the exit codes are what they are. |
| [`docs/profile-format.md`](docs/profile-format.md) | The shape of a profile bundle, and what the loader refuses. |
| [`docs/local-testing.md`](docs/local-testing.md) | Building a contract, running it locally, and what limits a `.wasm` target. |
| [`docs/testnet-testing.md`](docs/testnet-testing.md) | What this build does with a network target, and why. |
| [`docs/ci-integration.md`](docs/ci-integration.md) | Gating a pipeline on a conformance result, with a workflow example. |
| [`docs/certification.md`](docs/certification.md) | What a receipt commits to, and what it does not establish. |
| [`docs/security.md`](docs/security.md) | What conformance does not prove, and how untrusted input is handled. |
| [`docs/troubleshooting.md`](docs/troubleshooting.md) | Each failure class, what it means, and what to do about it. |

`CONTRIBUTING.md` describes the workspace, the code standards and how to add an
assertion dimension. `CHANGELOG.md` carries the versioning policy, including what
counts as a breaking change for a tool whose output is a verdict.

## Current status

This section is kept accurate rather than aspirational. What exists and is tested today:

| Crate | State |
| --- | --- |
| `estamora-core` | **Implemented.** The error taxonomy, the six conformance statuses, the rule that reduces vector results to a verdict, and the exit-code contract. |
| `estamora-soroban` | **Implemented for local execution.** A deterministic host pinned to a declared ledger point; a contract placed in it either by registration from WebAssembly or an in-repository fixture, or by holding it from the instance ledger entry a network reports; invocation with outcome classification, event capture, and authorization as a scenario property with the demanded authorizations recorded. Interface inspection reads a compiled contract's `contractspecv0` spec section — the only source that describes a contract the runner has never seen — with a hand-written WebAssembly section reader and XDR decoder. Reads against a deployed contract are answered through `estamora-core`'s `World`, in the vector's own vocabulary, so a failing requirement names `alice` rather than a base-32 address. |
| `estamora-profile` | **Implemented.** Loads a bundle, refuses a specification format it cannot execute, refuses a manifest entry that escapes the bundle or a document that is oversized, parses all six documents into typed structures, checks every cross-reference between them, keeps a valid bundle's warnings rather than discarding them, and refuses a bundle stored under an identity other than the one it declares. |
| `estamora-vectors` | **Implemented.** Loads the corpus a profile declares — its own operation directories plus the shared families it consumes — checks every vector against the profile it was found under, and excludes a profile-independent vector whose method the profile does not implement rather than inventing a defect. |
| `estamora-certification` | **Implemented.** Issues a receipt committing to a result — the pinned profile and corpus digests, the target, the verdict, per-dimension tallies and the digest of the report — and verifies one. Verification answers two questions separately: whether the report shown is the one the receipt is about (no key needed), and who asserted it (a key the caller already trusts). A signature checked against a key carried in the same document is reported as *unattributed*, never as verified. No on-chain publishing is implemented, deliberately. |
| `estamora-report` | **Implemented.** Renders a run as JSON, Markdown and `JUnit`, with the JSON document mirroring the specification's report schema field for field. A report this crate produces validates against the published schema, asserted by a cross-repository test. Contract-supplied metadata is sanitised before it reaches a rendered document, and the identifier grammar the schema imposes is satisfied by normalising internal check names and keeping the original in the detail field. |
| `estamora-assertions` | **Implemented.** Interprets every value expression and predicate the format defines — comparisons, relative changes, aggregates over a resource set, arithmetic, composites — against a `World` trait that abstracts the execution environment, and evaluates all seven conformance dimensions for one vector: interface compatibility, authorization, events, behaviour, state, invariants and failure. A requirement that could not be evaluated produces an undecidable vector rather than a failed one. |
| `estamora-cli` | **Implemented.** The `estamora` binary and the engine behind it: resolves a target (an in-repository fixture, a `.wasm` artifact, or a deployed contract and network), places it in a host, builds the vector's declared world, seeds it through the fixture entry points, applies the vector's authorization plan, invokes the method, records the before-and-after worlds, evaluates all seven dimensions, reduces the run to a verdict, and renders it as text, JSON, Markdown or JUnit. Six commands: `run`, `inspect`, `profile`, `validate`, `report`, `certify`. |

A **deployed contract** is measured, not merely identified. `--contract <id> --network <net>`
resolves the identifier over RPC to the WebAssembly the contract is running, verifies that the
artifact hashes to the code hash the contract instance itself declares, and then measures those
bytes in the local host exactly as a `.wasm` file on disk is measured. No funded account is
needed, no transaction is submitted, and a shared ledger cannot change the answer half way
through a corpus.

It is not redeployed to get there. Resolving a contract reads its **instance ledger entry**
— the entry that carries the code hash it declares — and that entry also carries the instance
storage its constructor wrote. The code and the entry are therefore placed in a ledger built
from the network's own answer and the host is built from it, so the constructor never runs, the
contract is reachable at the identifier the network serves it at, and it is measured in the
configuration the network has rather than an empty one. A contract whose constructor takes
arguments is consequently measurable, which it would not be if the runner had to run one.

Every way of *not* reading a contract is an environment failure that exits `4` and never `1`:
an unreachable endpoint is `NETWORK_ERROR`; an identifier that does not exist is
`CONTRACT_RESOLUTION_ERROR` with `reason: contract-not-found`; a Stellar Asset Contract, whose
behaviour is the host's rather than a deployed artifact's, is `reason: host-implemented-contract`;
and an instance entry that disagrees with the artifact it was fetched with is
`reason: artifact-hash-mismatch`. A CI job retries them or files them as infrastructure and
never reads them as a contract that failed its profile. `--network` accepts `testnet` and
`mainnet`; any other endpoint is named with `ESTAMORA_RPC_URL`.

A `.wasm` file on disk has no instance entry, so that path registers the artifact instead,
which runs a constructor it can run. One that declares arguments nobody can supply is reported
as `reason: constructor-needs-arguments` rather than passed invented ones: fabricating them
would fabricate the very state the vectors are then measured against.

### Two facts about Soroban execution that shape the design

Both were established by running contracts against the SDK's test host, not assumed,
and both are pinned by tests in `estamora-soroban`.

**A missing method aborts exactly like a deliberate `panic!()`.** The host reports both
through one channel, so a call that failed is *not* evidence that the contract refused
it. Only a contract whose interface has been inspected can have its aborts read as
refusals, which is why interface inspection runs before the behavioural vectors and why
every observation records whether that inspection happened.

**A refused call leaves no observable trace.** Its events and its mutations are both rolled
back, which is a stronger statement than it first appears and cuts both ways. It means a
contract that emits or mutates *before* it refuses is indistinguishable from one that refuses
first, so "a refusal emits no success event" and "a refusal does not mutate state" are
requirements every contract satisfies at the transaction boundary. They remain correct
requirements — they are what a contract would violate on a chain that did not roll back — but
they carry no discriminating power here, and the fixture set says so rather than claiming a
defect it cannot demonstrate.

**An authorization is observable only from a call that completed.** The demanded principals
and the arguments their signatures covered are read from the host's record of what it
authenticated, and a refused call unwinds that record before it can be read. A refusal for a
missing signature and a refusal for an insufficient balance arrive through the same channel
and leave the same empty record. So an empty record means "nothing was authenticated" and
never "this contract demands no authorization", and the authorization dimension treats the
demanded principals as *unobservable* — reported as a finding on the vector — whenever the
call did not complete. What discriminates an unauthorized-call vector is its outcome: a
contract that skips `require_auth` accepts the call and fails the requirement, and one that
demands the wrong principal accepts the substituted signature and fails it for the same
reason.

### What the loader decides, and what it refuses to decide

It decides whether a profile is **executable**: well-formed, complete, self-consistent,
stored where it says it is, and written against a format this runner understands. A
broken cross-reference is an error rather than a warning, because a requirement that
names something the profile does not declare cannot be evaluated, and reporting a verdict
against a requirement that was silently not applied is the one failure mode this project
exists to prevent.

It does not decide whether a profile is **correct**. Whether a requirement is complete or
faithful to its upstream standard is a review question for the specification repository,
and a runner that second-guessed it would be running a different profile from the one that
was reviewed.

The runner re-checks what the specification's own validator already checks, deliberately.
A bundle is consumed by path, and a bundle reached by path may not be the revision that was
validated — it may be a working copy or a hand-edited fixture. Re-checking is defence in
depth, and the two cross-repository tests in `crates/estamora-profile/tests/` are what hold
the two validators in agreement: they load every bundle the specification publishes,
including the SEP-41 profile, and fail if either side changes shape.

### What the assertion layer decides, and what it refuses to decide

It decides, for one vector, what the contract did and which requirements it satisfied. The
seven dimensions are evaluated in a fixed order, and two of the orderings are requirements
rather than preferences: the interface is inspected first, because a refusal is only
readable once the method is known to exist, and behaviour is evaluated before invariants,
because a behavioural rule names the invariants it requires.

It does not decide whether a profile's requirements are the right ones, and it does not
report a requirement it could not evaluate as a failed one. A read the contract cannot
answer, arithmetic that left the representable range, or a value that is not comparable to
the one the requirement names all make the vector **undecidable** — `error`, contributing
nothing to a verdict — because blaming a contract for the runner's inability to measure it
is the single most damaging mistake this system could make.

Two applicability rules are worth knowing, because they decide whether a requirement is
enforced at all. A behavioural rule applies only to vectors whose expected outcome matches
the rule's kind, and, for a failure rule that names its failures, only to a vector that
constructs one of them; a rule that does not apply is recorded as inapplicable rather than
satisfied, since counting it as a pass would claim coverage the scenario never exercised.
An invariant outside its declared scope is inapplicable in the same way.

## Installing it

Three routes, and they answer a different question about which revision you are running.
Pick by what you need to be able to say afterwards.

### A released binary

```bash
curl -fsSL https://raw.githubusercontent.com/Estamora-Soroban-Layers/estamora-conformance-runner/v0.1.3/scripts/install-binary.sh | sh
```

The script detects the platform, downloads the matching archive from the release, checks it
against the release's `SHA256SUMS`, and installs it to `~/.cargo/bin` or `~/.local/bin`. It
**refuses to install anything it cannot verify** — no checksum file, no entry for the
archive, or a mismatch all stop the install rather than printing a warning and continuing.
That is not ceremony: an installer that fetches a binary and runs it unchecked is trusting
the network, a DNS answer and a TLS certificate for the integrity of the tool whose job is
to be trustworthy about somebody else's contract.

| Platform | Archive |
| --- | --- |
| Linux, x86-64 | `estamora-x86_64-unknown-linux-gnu.tar.gz` |
| Linux, arm64 | `estamora-aarch64-unknown-linux-gnu.tar.gz` |
| macOS, Apple silicon | `estamora-aarch64-apple-darwin.tar.gz` |
| Windows, x86-64 | `estamora-x86_64-pc-windows-msvc.tar.gz` |

`--version v0.1.3` pins a release instead of taking the latest, `--target` overrides the
detected platform, and `--dir` chooses where it lands. Every archive also carries its own
`README`, its licence and a `VERSION` file, so what was shipped and what the tool reports
cannot disagree.

### From a tagged revision, with `cargo`

```bash
cargo install --locked --git https://github.com/Estamora-Soroban-Layers/estamora-conformance-runner \
  --tag v0.1.3 estamora-cli
```

This compiles from source, so it needs a Rust toolchain and several minutes, and in exchange
the binary is built from a revision you can read. It does **not** use the toolchain pinned in
`rust-toolchain.toml`: `cargo install` compiles the checkout in a temporary directory, and
rustup selects a toolchain from the working directory, not from the source tree. The released
binaries above are built with the pinned toolchain and are the ones whose identity matches
what a report records.

No crate is on crates.io yet. The seven library crates are publishable and `estamora-cli`
is not one of them and will not become one. It links `estamora-fixture-token`, the contract that `--contract
fixture:<name>` deploys so the runner can exercise its own execution path without a deployed
contract, and that fixture is deliberately `publish = false`. Packaging rewrites a path
dependency into a registry requirement of the same version, so the requirement the command
would carry names a version no registry will hold. The two routes above are how it arrives,
and `cargo install estamora-cli` is not a third one to wait for.

Publication is deliberately not one of the three routes. `cargo publish` is a human command
against a token rather than a workflow step, because a version on crates.io cannot be
withdrawn and this is the one place in the repository where a permanent external action
should not be a consequence of pushing a tag. The sequence it has to run in is written once,
in the publish block at the end of `.github/workflows/release.yml`, and checked on every
commit by `scripts/check-publish-plan.py`, so the plan cannot name a crate that has no way of
being published. Until that command is run, a released binary or a checkout is how `estamora`
is obtained, and no registry command is a route to it.

### From a checkout

```bash
./scripts/install.sh
```

Installs the working tree you are standing in, which is what a contributor wants and the
wrong answer for anybody who has to ask which revision produced a verdict.

### It also needs a specification checkout

A profile is not compiled into the binary. `--profile sep-41@1.0` resolves inside a checkout
of [`estamora-conformance-spec`](https://github.com/Estamora-Soroban-Layers/estamora-conformance-spec),
because which revision of which profile a verdict was produced against is part of what the
verdict means — a bundled copy would let the two drift silently.

```bash
git clone https://github.com/Estamora-Soroban-Layers/estamora-conformance-spec
export ESTAMORA_SPEC_REPO="$PWD/estamora-conformance-spec"
```

The checkout is found from `--spec`, then `ESTAMORA_SPEC_REPO`, then the sibling directory.
A binary installed from a release is a sibling of nothing, so without one of the first two
`estamora` stops with a message naming the variable, the directory it tried and the command
that clones it.

## Running it

```bash
# Measure a contract against a profile and print a verdict.
estamora run --profile sep-41@1.0 --contract fixture:none

# The same run as the normative document, for another tool to consume.
estamora run --profile sep-41@1.0 --contract fixture:none --format json --out report.json
```

From a checkout of this repository, `cargo run -p estamora-cli -- run …` is the same binary
without installing it.

The in-repository fixtures are contracts that are wrong in exactly one way each, so a
`NON_CONFORMANT` result can be traced to a single named requirement:

| Fixture | Wrong in | Noticed by |
| --- | --- | --- |
| `none` | Nothing — the conforming case | `CONFORMANT`, exit `0` |
| `skips-authorization` | `transfer` never calls `require_auth` | the authorization plan, the refusal, the events and the state |
| `omits-event` | moves the value, publishes nothing | the event cardinality and value requirements |
| `wrong-credit-amount` | credits one unit less than it debits | the balance deltas and the conservation invariant |
| `double-emits` | two transfer events for one movement | the event cardinality requirement |
| `wrong-event-amount` | states an amount one unit larger | the event data requirement |
| `allows-overdraft` | permits a negative balance | the must-fail rule, the non-negative bound, and the state assertions |
| `missing-decimals` | the interface omits `decimals` | the interface dimension, on every vector |

The other five commands each answer one question and exit with the same code contract:
`inspect` reads a contract's interface without executing anything against it, `profile`
describes what a profile requires, `validate` checks a profile and its corpus without
executing anything, `report` renders a stored report, and `certify` issues or verifies a
certification receipt.

For CI, the two flags a pipeline needs are `--format junit --out results.xml` for a gate it
already understands, and `--no-fail`, which exits `0` whatever the verdict while still
recording it — for a pipeline that wants the report of a non-conformant contract without
failing the job that produced it.

This repository is also usable as a GitHub Action, which builds the tool from the revision
the `uses:` reference pins and fails the job when the verdict is not `CONFORMANT`:

```yaml
- uses: Estamora-Soroban-Layers/estamora-conformance-runner@v1
  with:
    profile: sep-41@1.0
    contract: ${{ vars.CONTRACT_ID }}
    network: testnet
    format: junit
    report: estamora-report.json
```

It reports `status`, `exit-code` and `report` as outputs, so a later step can branch on the
verdict rather than on a bare failure. [`docs/ci-integration.md`](docs/ci-integration.md)
has the full example, including the two cases that must not be read as a contract defect.

## Has this been run against a contract it did not write?

Yes, and the report is committed rather than described.
[`examples/testnet-contract/report.json`](examples/testnet-contract/report.json) is the
output of a measurement made over RPC against a contract deployed to testnet: the
identifier resolved to the WebAssembly in its instance ledger entry, the artifact verified
against the code hash that entry declares, the interface read out of the deployed bytes,
and **63 checks across all seven dimensions reported, 0 failed**. One of the twenty vectors
was decided there and then, because it needs no seeded state; the other nineteen cannot be,
which is why that run is `INCONCLUSIVE` and not conformant. The example's README records
the identifiers, the digests and the command, and says what the verdict does and does not
mean.

## Building

The toolchain is pinned in `rust-toolchain.toml` and is part of the runner's identity: a
result is only meaningful if the tooling that produced it is identified.

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Test coverage

Measured with `./scripts/coverage.sh`, which runs `cargo llvm-cov` over the workspace and fails
below the floor. The scope is the crates this project ships:

```
TOTAL   13963 regions   73.52%   |   9790 lines   73.32%   |   963 functions   79.34%
```

**73.3% of the runner's own lines**, over 9,790 lines of Rust, measured with `cargo llvm-cov`.
The floor is 70%, enforced by the script and by the `test coverage` job in CI.

What is measured, and what is not, because the denominator is what makes the figure mean
something:

| Path                  | In the figure | Why                                                                                    |
| --------------------- | ------------- | -------------------------------------------------------------------------------------- |
| `crates/`             | **yes**       | The runner: the measurement engine, the assertion engine, the report, the CLI.        |
| `fixtures/contracts/` | no            | Deliberately-defective Soroban contracts used as *inputs*. One of them is the free-mint bug the worked example is built around. They are subject matter, not tooling. |
| `integration-tests/`  | no            | Test code.                                                                             |
| `benches/`            | no            | Benchmark harnesses.                                                                    |

Excluding them is not a way of raising the number: including every path, the workspace reports
74.2%, which is *higher* than the product figure because the integration tests are well covered.
The product number is the honest one to quote, and it is the lower of the two.

### Where the gap is

One finding is worth publishing rather than leaving in a report. **`crates/estamora-cli/src/commands/`
is at 0.00%** — 343 instrumentable lines, 658 regions and 32 functions, across seven files
totalling 931 lines of source (`run.rs`, `certify.rs`, `inspect.rs`, `profile.rs`, `report.rs`,
`validate.rs`, `mod.rs`). Those handlers are thin and they are exercised end to end by the
exit-code contract tests that build the binary and run it, but no test drives them in-process, so
`cargo llvm-cov` cannot see them. The same shape of gap as a spawned subprocess in any language,
and it is the most valuable next thing to close: moving the argument handling and the diagnostic
translation of those handlers behind callable functions would put roughly 340 lines inside the
measured surface.

The lowest-covered modules that *are* measured are `estamora-cli/src/engine/values.rs` (56.1% line)
and `estamora-assertions/src/dimensions/invariants.rs` (61.9% line) — both reachable in-process, both worth
a focused test module, and both named in the open issues rather than left as a surprise.

## The exit-code contract

A CI system reads a process exit code and nothing else, so the mapping from a verdict to an
exit code is fixed here, implemented in `estamora-core`, and tested for every branch.

| Code | Meaning | Who is at fault |
| --- | --- | --- |
| `0` | `CONFORMANT` | Nobody |
| `1` | `NON_CONFORMANT` or `PARTIALLY_CONFORMANT` | The contract |
| `2` | `INCONCLUSIVE` — the suite could not decide | The run |
| `3` | `PROFILE_ERROR` — the requirements were unusable | The specification |
| `4` | `EXECUTION_ERROR` — the environment failed | The environment |
| `5` | The runner itself failed | The runner |
| `64` | The command line was wrong | The invocation |

The distinction that matters most is between `1` and `4`. A runner that reports an
unreachable node, an unbuilt fixture or a refused connection as a contract failure blocks a
release for a network outage and teaches its users to distrust every verdict it produces.
Only a violated requirement may produce `1`.

## What Estamora does not do

**Passing a profile is not evidence that a contract is secure.** Estamora checks defined
behavioural compatibility, and nothing else. It does not replace formal verification, a
security audit, penetration testing, economic analysis or vulnerability research. A profile
describes the behaviour a standard requires; it does not enumerate an implementation's
mistakes, and a contract can be fully conformant and still exploitable.

Estamora is not a security scanner, an explorer, a wallet, a token dashboard, a registry or
a general-purpose contract testing framework, and it is not intended to become one.

## License

Apache-2.0. See [`LICENSE`](LICENSE).
