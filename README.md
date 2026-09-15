# estamora-conformance-runner

Estamora answers one question about a Soroban contract:

> Does this deployed contract actually behave according to the standard or profile it claims
> to implement?

Exposing the expected methods is not the answer. A contract can implement `transfer`,
`approve` and `balance` with exactly the right signatures and still move the wrong amount,
credit the wrong account, skip the authorization check, emit no event, or return an error
after mutating state. This repository executes a contract against the requirements defined
in the specification repository and reports, per assertion, whether it conformed.

## Where this sits

Estamora is exactly two repositories.

| Repository | Owns |
| --- | --- |
| [`estamora-conformance-spec`](https://github.com/Estamora-Soroban-Layers/estamora-conformance-spec) | What conformance *means*: profiles, vectors, schemas and the validation tooling |
| `estamora-conformance-runner` (this repository) | Measuring a contract against those requirements and reporting the result |

The specification repository is normative. This repository contains no requirements of its
own: it loads a profile, executes the vectors that belong to it, and reports what happened.
Where this runner and the specification could disagree, the specification is right and this
repository has a defect.

## Current status

This repository is under construction, and this section is kept accurate rather than
aspirational. What exists and is tested today:

| Crate | State |
| --- | --- |
| `estamora-core` | **Implemented.** The error taxonomy, the six conformance statuses, the rule that reduces vector results to a verdict, and the exit-code contract. |
| `estamora-soroban` | **Implemented for local execution.** A deterministic host pinned to a declared ledger point, contract registration from WebAssembly or an in-repository fixture, invocation with outcome classification, event capture, and authorization as a scenario property with the demanded authorizations recorded. Interface inspection reads a compiled contract's `contractspecv0` spec section — the only source that describes a contract the runner has never seen — with a hand-written WebAssembly section reader and XDR decoder. Reads against a deployed contract are answered through `estamora-core`'s `World`, in the vector's own vocabulary, so a failing requirement names `alice` rather than a base-32 address. |
| `estamora-profile` | **Implemented.** Loads a bundle, refuses a specification format it cannot execute, refuses a manifest entry that escapes the bundle or a document that is oversized, parses all six documents into typed structures, checks every cross-reference between them, keeps a valid bundle's warnings rather than discarding them, and refuses a bundle stored under an identity other than the one it declares. |
| `estamora-vectors` | **Implemented.** Loads the corpus a profile declares — its own operation directories plus the shared families it consumes — checks every vector against the profile it was found under, and excludes a profile-independent vector whose method the profile does not implement rather than inventing a defect. |
| `estamora-certification` | **Implemented.** Issues a receipt committing to a result — the pinned profile and corpus digests, the target, the verdict, per-dimension tallies and the digest of the report — and verifies one. Verification answers two questions separately: whether the report shown is the one the receipt is about (no key needed), and who asserted it (a key the caller already trusts). A signature checked against a key carried in the same document is reported as *unattributed*, never as verified. No on-chain publishing is implemented, deliberately. |
| `estamora-report` | **Implemented.** Renders a run as JSON, Markdown and `JUnit`, with the JSON document mirroring the specification's report schema field for field. A report this crate produces validates against the published schema, asserted by a cross-repository test. Contract-supplied metadata is sanitised before it reaches a rendered document, and the identifier grammar the schema imposes is satisfied by normalising internal check names and keeping the original in the detail field. |
| `estamora-assertions` | **Implemented.** Interprets every value expression and predicate the format defines — comparisons, relative changes, aggregates over a resource set, arithmetic, composites — against a `World` trait that abstracts the execution environment, and evaluates all seven conformance dimensions for one vector: interface compatibility, authorization, events, behaviour, state, invariants and failure. A requirement that could not be evaluated produces an undecidable vector rather than a failed one. |
| `estamora-cli` | **Implemented.** The `estamora` binary and the engine behind it: resolves a target (an in-repository fixture, a `.wasm` artifact, or a deployed contract and network), deploys it, builds the vector's declared world, seeds it through the fixture entry points, applies the vector's authorization plan, invokes the method, records the before-and-after worlds, evaluates all seven dimensions, reduces the run to a verdict, and renders it as text, JSON, Markdown or JUnit. Six commands: `run`, `inspect`, `profile`, `validate`, `report`, `certify`. |

One target kind is deliberately not implemented: a **deployed contract**. Resolving one needs a
Soroban RPC transport, and this build links none. Rather than accept a contract identifier and
produce a verdict from nothing, `--contract <id> --network <net>` fails as
`CONTRACT_RESOLUTION_ERROR` with `reason: network-transport-unavailable` and exit code `4` —
classified as an environment failure, never as a contract that failed its profile. The failure
mode is the same one an unreachable endpoint produces on a build that *does* have a transport,
so the pipeline is exercised against it either way.

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

## Running it

The specification checkout is found from `--spec`, then `ESTAMORA_SPEC_REPO`, then the
sibling directory, because the two repositories are developed beside each other and CI checks
both out into one workspace.

```bash
# Measure a contract against a profile and print a verdict.
cargo run -p estamora-cli -- run --profile sep-41@1.0 --contract fixture:none

# The same run as the normative document, for another tool to consume.
cargo run -p estamora-cli -- run --profile sep-41@1.0 --contract fixture:none \
  --format json --out report.json
```

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

## Building

The toolchain is pinned in `rust-toolchain.toml` and is part of the runner's identity: a
result is only meaningful if the tooling that produced it is identified.

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

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
