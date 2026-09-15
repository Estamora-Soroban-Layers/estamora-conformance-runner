# Local testing

A local run executes a contract in a real Soroban host, on this machine, with no
network involved. It is the default and the mode everything else is built on.

## The three targets

`--contract` accepts three things, and they are resolved differently:

| Spelling | What it is |
| --- | --- |
| `fixture:<name>` | One of this repository's own contracts, registered from its Rust type. |
| `<path>.wasm` | A compiled contract artifact, deployed from its bytes. |
| `<contract-id> --network <net>` | A deployed contract. Its WebAssembly is read over RPC and then measured locally; see `docs/testnet-testing.md`. |

`fixture:` targets exist so that a run can be attempted before you have written
anything. The fixtures are the reference token with exactly one defect injected
each; an unknown name is refused by name and the refusal lists the fixtures that
exist, rather than silently defaulting to the conforming one.

## Building a contract to measure

A Soroban contract is compiled for WebAssembly, and a normal host build will not
load:

```console
$ cargo build --target wasm32v1-none --release -p my-token
$ ls target/wasm32v1-none/release/my_token.wasm
```

`wasm32v1-none` is the current target for Soroban contracts. On older toolchains the
same artifact comes from `wasm32-unknown-unknown`. `rust-toolchain.toml` in this
repository pins `wasm32v1-none`, so that a checkout can build a contract for these
examples without adding the target yourself. Nothing in the runner's own workspace is
built for it — a fixture is registered from its Rust type, which is explained under
*The local host is not a mock* — but one fixture is: see *A compiled fixture* below.

The runner reads the contract's interface from the artifact's `contractspecv0`
section **before** deploying it. An artifact that publishes no spec section is
refused with a contract resolution error, rather than deployed and then asked what
it exposes: an artifact with no declared interface cannot be measured against a
profile's interface requirements at all, and reporting an empty interface would
claim a reading that never happened.

## What limits a `.wasm` run, and why

A vector declares the world its operation runs against — opening balances,
allowances, total supply. Against a fixture, the runner establishes that world by
calling the fixture's own setup entry points.

**It does not assume your contract publishes equivalent entry points**, and it does
not write your contract's storage directly. So a vector that declares an opening
balance or an allowance against a `.wasm` target is reported as **`skipped`**, with a
diagnostic naming the reason:

```text
• transfer-moves-exact-amount (skipped)
    · seeding-unavailable the vector declares an opening balance or allowance and a
      compiled artifact is not assumed to publish Estamora's fixture setup entry
      points, so a vector whose world declares no opening balance or allowance can be
      prepared and one that declares either cannot; the requirement was not exercised
      and this vector contributes nothing to the verdict
```

A skipped vector is not a pass and not a failure, and what the run concludes depends on
whether anything else was decided:

| What was decided | Status | Exit |
| --- | --- | --- |
| Some vectors decided, at least one required vector skipped | `INCONCLUSIVE` | `2` |
| Nothing decided at all | `EXECUTION_ERROR` | `4` |

The second row is the ordinary result of measuring a deployed artifact: no state can be
established in one, so no vector can be decided, and the honest answer is not a
conformance result but an environment failure. What a run must never do is report
`CONFORMANT` because the vectors it could not prepare simply did not fail. The
alternative — measuring the contract against a world it was never put into — would
produce a verdict that looks exactly like a real one and is not one.

This is a real limitation and it is worth stating plainly: **against an arbitrary
`.wasm`, Estamora can measure the vectors whose starting world needs no opening
state.** For SEP-41 that is `unknown-account-balance-is-zero` and the metadata
reads. Everything else needs the contract to hold a balance to begin with, and
SEP-41 has no method that establishes one — minting is outside the interface the
standard defines, precisely because who may mint is the issuer's decision and not
the standard's.

Establishing opening state for an arbitrary contract therefore needs a declaration
the specification format does not currently have: a way for a vector to say *call
this method, with this authorization, to reach this state*, and for a profile to
declare which methods may be called that way. That is a change to the specification
— which is where it belongs, since this repository has no authority to decide it —
and it is not something this runner will invent on its own. Until it exists, a
contract being measured locally is measured through the fixture target, or through a
profile whose vectors need no opening state.

### The second limit: a constructor that takes arguments

A `.wasm` file is deployed by registering it, and registering an artifact runs its
`__constructor`. The environment has no arguments to give it, and a constructor's
arguments are the deployer's decision — an admin address, a fee recipient, a decimal
scale. The runner will not invent them, because a contract placed in a state the
runner made up is no longer the contract anybody deployed, and every requirement
measured against it would be measured against that invention.

So an artifact whose constructor declares arguments is refused, by name and with the
arguments it wanted:

```console
CONTRACT_RESOLUTION_ERROR: the artifact ./my_token.wasm declares a constructor
 taking admin: address, decimal: u32, and a constructor can only be run by whoever
 deployed the contract: only the deployer knew what to pass it. The runner will not
 invent arguments, because doing so would fabricate the very state the vectors are
 then measured against. No instance can be created locally, so nothing about this
 contract's behaviour was observed
  reason: constructor-needs-arguments
```

That is a resolution failure — exit `4`, the environment at fault — and not a verdict
about the contract. An artifact with no constructor, or one that takes no arguments,
is registered and measured normally.

This limit belongs to the `.wasm` path specifically. A contract read from a network is
never registered: its deployed instance ledger entry is placed in the ledger instead,
so its constructor — whatever its arguments — is never run. See
`docs/testnet-testing.md`.

## The local host is not a mock

The hosts are real and so is the contract code. `fixtures/contracts/` are written
against the same Soroban SDK a user's contract is, and they are executed by the same
host that executes a deployed contract — the runner never substitutes a mock for it.
What is pinned is time: the ledger sequence, the close time and every account's
authorization are **fixed by the vector**, which is what makes a run reproducible. Two
runs of the same vector produce the same events, the same balances and the same
verdict.

A fixture is *registered from its Rust type* rather than deployed from WebAssembly,
which is a real difference from the other two targets and has two consequences worth
knowing. Its code is compiled natively rather than to Wasm, so it does not pass through
the VM a deployed contract runs in; and it has no artifact to inspect, so it declares
its own interface in the same shape that inspection produces from a real one — the
reasoning is in `fixtures/contracts/conformance-token/src/interface.rs`. That is why
`cargo test --workspace` needs neither a network nor a WebAssembly build: the fixtures
are Rust crates in the workspace and the tests that measure them compile them as part
of the build.

What that does *not* change is the execution environment. It is the same host with
time and randomness pinned, and it is why a stored report can be re-read and
re-rendered a year later and produce the same document — which `integration-tests/`
asserts against a report whose timestamp is pinned. `docs/testnet-testing.md` covers
what changes when a real network is involved.

Two facts about the host shape the design and are worth knowing before reading the
source:

* **A refused call unwinds the host's record of what was authenticated.** There is
  nothing left to compare an authorization requirement against after a refusal,
  which is why the authorization dimension reports a finding rather than a pass for
  a vector that expects a refusal. See `docs/execution-engine.md`.
* **Reading a contract's interface is a prerequisite for a call.** The runner reads
  the spec section before anything else, so a call that later aborts can be
  attributed to the contract rather than to a mis-typed invocation. The interface is
  read first, and the invocation is typed against what was read.

## A compiled fixture

The suite measures one contract as a **compiled artifact** as well as from its Rust type:
`fixtures/contracts/measurable-token` declares its own workspace so that it can be built
for WebAssembly, and `scripts/build-fixture-wasm.sh` produces the artifact under
`fixtures/wasm/` that the tests deploy. That artifact is committed, so measuring it needs
no build step, and CI rebuilds it and fails if the committed copy differs.

It exists because of what could not otherwise be tested. A registered Rust type has no
`contractspecv0` section, so interface inspection reads a declared interface instead of a
real one, and a `.wasm` target has no artifact at all. Measuring this one exercises both,
and pins the two answers a run against a deployed contract has to get right: the digest a
report records is the digest of the bytes, and a run that could not prepare a single
scenario reports an environment failure rather than a verdict.

## A local run, end to end

```console
# Point at a specification checkout.
$ export ESTAMORA_SPEC_REPO=/path/to/estamora-conformance-spec

# Say what a profile requires, without executing anything.
$ estamora profile --profile sep-41@1.0

# Check that the profile and its corpus are usable.
$ estamora validate --profile sep-41@1.0

# Read a contract's interface.
$ estamora inspect --contract ./target/wasm32v1-none/release/my_token.wasm

# Measure it.
$ estamora run \
    --profile sep-41@1.0 \
    --contract ./target/wasm32v1-none/release/my_token.wasm \
    --report report.json
```

If you have no contract yet, `--contract fixture:none` runs the whole pipeline
against the reference token. That is what `examples/local-contract/` walks through,
including the real output.

## Running the test suite

```console
$ cargo test --workspace
```

The workspace suite needs no network and no contract to be built by hand: the
fixture contracts are Rust crates in the workspace, so the tests that measure them
compile them as part of the build. Nothing in `cargo test --workspace` reaches a
network, and nothing requires a specification checkout. The tests that measure the
real SEP-41 profile run only when `ESTAMORA_SPEC_REPO` is set and the checkout
actually contains it, and they skip themselves rather than fail when it is absent.

```console
$ cargo test -p estamora-integration-tests      # the end-to-end suite
$ cargo test -p estamora-soroban                # the execution layer
$ cargo test -p estamora-assertions             # the rule evaluators
```

## Where to look when something is wrong

`docs/troubleshooting.md` maps each error class to what it means and what to do
about it. The short version: the class name is the diagnosis, and the four classes
never share an exit code, so a pipeline can tell a broken profile from a broken
contract from a broken environment without parsing a message.
