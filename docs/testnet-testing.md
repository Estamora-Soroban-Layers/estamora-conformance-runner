# Testnet execution

Measuring a deployed contract is different from measuring an artifact, and the
difference is not the contract: it is that the contract is no longer yours, it may
have been upgraded since you read its source, and the ledger it runs on is not one
you control.

## What this build does today

`--contract <id> --network <name>` is accepted, modelled, and refused:

```console
$ estamora run --profile sep-41@1.0 --contract CDLZ…GCYSC --network testnet
CONTRACT_RESOLUTION_ERROR: contract CDLZ…GCYSC on `testnet` cannot be resolved by this
build: reading a deployed contract's interface and state needs a Soroban RPC transport,
and none is linked into this runner. No verdict about the contract was reached, and this
failure is classified as an environment failure rather than as a conformance result
  contract: CDLZ…GCYSC
  network: testnet
  reason: network-transport-unavailable
$ echo $?
4
```

That is deliberate, and it is deliberately not graceful. A runner that accepted a
contract identifier, did nothing with it, and printed a verdict would be the worst
possible outcome: a fabricated `CONFORMANT` is indistinguishable from a real one to
anyone who did not run it. So the failure names its reason and exits `4`, which is
the code for *the environment failed, retry*, and never `1`.

A network failure and a non-conformant contract have nothing in common, and a CI job
that conflated them would train its users to ignore it. `integration-tests/testnet/`
pins exactly that property, along with the classification.

## Why the transport is absent rather than stubbed

Reading a deployed contract needs a Soroban RPC client, and no crate providing one
is available to this workspace. The alternative to an honest absence was a dependency
that does not compile, or a module that looks implemented and returns mocked data.
Both are worse than a named refusal, because both make an unimplemented path look
implemented — and a mocked network read feeding a real-looking verdict is the failure
mode this project exists to prevent.

The workspace manifest states this where the dependency would have been declared, so
a reader does not have to discover it from a missing crate.

## What is already in place for it

The network path is a **boundary**, not a second pipeline. Everything except the
transport is written and tested:

* `Target::Remote` models a contract identifier together with its network, and the
  exit classification for it is `EnvironmentFailed`.
* The report records `network`, the contract identifier and the Wasm hash where one
  is available, so a verdict names what it is about. A conformance result that does
  not name the contract is not evidence of anything.
* Interface inspection reads a contract's declared interface from a compiled
  artifact today, and the same inspection model is what a remote resolution would
  populate — including the rule that an interface is one layer of conformance and
  never a verdict.
* The local host is the same execution environment with time and authorization
  fixed, so a network run adds a transport and a ledger, not a second evaluation.

The consequence is that adding network support is adding one transport, not a new
code path, and the report shape a network run would produce is the one already
generated.

## What a network run would have to do

In the order the pipeline already establishes:

1. **Resolve** — fetch the contract's Wasm hash or its contract instance, and refuse
   explicitly when the identifier is unknown on that network. An unreachable node is
   `NETWORK_ERROR`; an identifier that does not exist is
   `CONTRACT_RESOLUTION_ERROR`. Neither is a verdict.
2. **Inspect** — read the declared interface from on-chain metadata. A contract
   whose metadata cannot be read has not been measured; reporting an empty interface
   would claim a reading that did not happen.
3. **Seed** — the same difficulty local runs have, one step harder: a deployed
   contract's starting state cannot be set by the runner at all. A vector whose world
   declares an opening balance is only measurable if the contract can be put into
   that state through its own interface. Today it is reported `skipped` with the
   reason; see `docs/local-testing.md`.
4. **Invoke and observe** — a transaction's events and its authorization are the same
   things a local run observes, read from a transaction result rather than from a
   host.
5. **Report** — with the network, the contract identifier and the ledger sequence the
   run was made at, because a conformance result is about a contract at a moment.

## Keeping network tests out of the default suite

Network tests are opt-in and separate, for three reasons: `cargo test --workspace`
must not depend on a node being up, a flaky connection must not look like a
regression, and a test that needs a funded account must not need one to check a rule
about event cardinality.

```console
$ ./scripts/test-testnet.sh            # nothing runs without ESTAMORA_TESTNET_ENABLED
```

The script does nothing unless it is explicitly enabled and a specification checkout
is present, and it reports that it did nothing rather than passing quietly. A suite
that reports success because it ran nothing is worse than one that fails.

`integration-tests/testnet/` holds the tests that do not need a network: the target
resolution, the classification of a network failure, and the exit code it produces.
Those run in the default suite, because they assert something true of every build.
