# Measuring a deployed contract

The situation: a contract has been deployed to a network, someone claims it
implements a standard, and you want to know whether that is true *of the deployed
artefact* rather than of the source you happen to have checked out.

## The command

```console
$ estamora run \
    --profile sep-41@1.0 \
    --contract CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC \
    --network testnet \
    --format junit --out conformance.xml \
    --report conformance.json
```

`--contract` with a contract identifier requires `--network`; the identifier
alone does not say where it lives, and a runner that guessed would be measuring
something other than what you asked about. The report records both, along with
the source account the invocation was made from, because a conformance result that
does not name the contract it is about is not evidence of anything.

## What the runner does with that command

The identifier is resolved over RPC to the WebAssembly the contract is running, and
those bytes are then measured in the local host — the same host, and the same
evaluation, as a `.wasm` file on disk:

```console
$ estamora run --profile sep-41@1.0 --contract CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC --network testnet
CONTRACT_RESOLUTION_ERROR: CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC on
`testnet` is a Stellar Asset Contract, whose behaviour is implemented by the host rather
than by a deployed artifact. There is no WebAssembly to measure, so no portability
profile can be applied to it
  contract: CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC
  network: testnet
  reason: host-implemented-contract
$ echo $?
4
```

That contract is a poor example for a different reason than it used to be: it is the
native asset's Stellar Asset Contract, and a Stellar Asset Contract is the one kind of
contract that genuinely cannot be measured this way — its behaviour lives in the host,
not in an artifact that could be deployed anywhere. The runner says so by name rather
than reporting an empty interface.

Nothing is submitted to the network and no account is funded. Two calls are made,
`getNetwork` and `getLedgerEntries`. A verdict must not depend on a shared ledger's
state at the moment it was asked, on a node's willingness to accept a transaction, or
on somebody else's keys, and re-running it produces the same result as long as the
deployment has not changed.

Resolution verifies the one property that makes reading a contract from a single server
mean anything: the fetched WebAssembly must hash to the code hash the contract instance
itself declares. A disagreement is `reason: artifact-hash-mismatch` and exits `4`.

`integration-tests/testnet/` pins the property that matters most — that an outage is an
environment failure and never a verdict:

```console
$ cargo test -p estamora-integration-tests --test testnet
```

The tests that need a live ledger are `#[ignore]`d and run through
`scripts/test-testnet.sh`; the classification itself is asserted by default, by pointing
a known network name at a closed port. A network failure and a non-conformant contract
have nothing in common, and a CI job that conflated them would train its users to ignore
it.

## Networks without a live node

For the parts of the pipeline that must be exercised deterministically, the local
ledger *is* the abstraction: `fixtures/contracts/` are real contract code, run in a real
Soroban host, with the ledger sequence, the timestamp and every account's authorization
fixed by the vector. That is not a mock of a network — it is the same execution
environment, with time and randomness pinned — and it is why a stored report can be
re-read and produce the same verdict a year later.

Testnet execution is therefore opt-in and separate from the default suite. It is
never a reason for the ordinary `cargo test` to require a network, and `scripts/`
carries the runner for it separately.
