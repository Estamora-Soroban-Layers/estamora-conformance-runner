# Testnet execution

Measuring a deployed contract is different from measuring an artifact, and the
difference is not the contract: it is that the contract is no longer yours, it may
have been upgraded since you read its source, and the ledger it runs on is not one
you control.

## How a deployed contract is measured

`--contract <id> --network <name>` resolves the identifier to the WebAssembly the
contract is running, and then measures those bytes in the local host — the same host,
and the same evaluation, as a `.wasm` file on disk:

```console
$ estamora run --profile sep-41@1.0 --contract C… --network testnet
```

Nothing is submitted to the network and no account is funded. Two calls are made,
`getNetwork` and `getLedgerEntries`, and everything a verdict depends on happens
locally. That is deliberate: a verdict must not depend on a shared ledger's state at the
moment it was asked, on a node's willingness to accept a transaction, or on somebody
else's keys. Re-running it produces the same result as long as the deployment has not
changed.

Resolution verifies one property that makes reading a contract from a single server
mean anything at all: the fetched WebAssembly must hash to the code hash the contract
instance itself declares. Without that check the two ledger entries could disagree and
the runner would measure an artifact that is not the one deployed. A disagreement is
`reason: artifact-hash-mismatch` and exits `4`.

### The three ways resolution fails

| Failure | Class | Reason | Exit |
| --- | --- | --- | --- |
| A node that cannot be reached | `NETWORK_ERROR` | `network-unreachable` | 4 |
| A well-formed identifier that is not on that network | `CONTRACT_RESOLUTION_ERROR` | `contract-not-found` | 4 |
| A Stellar Asset Contract | `CONTRACT_RESOLUTION_ERROR` | `host-implemented-contract` | 4 |
| An artifact whose constructor takes arguments | `CONTRACT_RESOLUTION_ERROR` | `constructor-needs-arguments` | 4 |

None of these is a verdict, and none exits `1`. A network failure and a non-conformant
contract have nothing in common, and a CI job that conflated them would train its users
to ignore it.

## The constructor boundary

Instantiating an artifact runs its `__constructor`, and no environment supplies the
arguments a deployed contract's constructor took — only its deployer knew them. The
runner refuses such an artifact by name rather than inventing plausible arguments,
because inventing them would fabricate the very state the vectors are then measured
against:

```console
CONTRACT_RESOLUTION_ERROR: contract CAYPA…HXH2JA on `testnet` as read from
https://soroban-testnet.stellar.org declares a constructor taking admin: address,
fee_recipient: address, and a constructor can only be run by whoever deployed the
contract: only the deployer knew what to pass it.
  reason: constructor-needs-arguments
```

This is the honest boundary of local re-execution, and it is a real one: a contract
whose constructor takes arguments cannot be instantiated locally, so nothing about its
behaviour can be concluded from this runner. A contract with no constructor, or one
that takes none, is measured normally.

## Seeding is unavailable on a network target

A vector declares the state its operation starts from: Alice holds 1000. For that to be
true, something has to put 1000 there. A deployed contract publishes its own interface,
not Estamora's fixture setup entry points, and a measurement does not write to the
ledger it reads from — so a vector whose world declares an opening balance or an
allowance is reported `skipped` with that reason rather than failed. A requirement that
was not exercised is not a requirement that was met; see `docs/local-testing.md`.

## Naming an endpoint this build does not know

`testnet` and `mainnet` are the two networks this build knows by name, and deliberately
so: a short list of networks a conformance result is meaningful about, rather than a
registry. Any other endpoint is named explicitly, which makes the dependency on that
particular operator visible in the command that produced the result:

```console
$ ESTAMORA_RPC_URL=https://soroban-rpc.example.org \
    estamora run --profile sep-41@1.0 --contract C… --network futurenet
```

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

The classification that matters most — that an outage is an environment failure and
never a verdict — needs no network to assert, and is asserted in two places that do
run by default: `estamora-soroban`'s `tests/testnet.rs` and `estamora-cli`'s
`engine::target` tests both point a known network name at a closed port and check the
class, the blame, the exit code and the endpoint the message names. What is left for
the ignored set is the part that genuinely needs a live ledger: that a real deployment
resolves to the artifact it declares.
