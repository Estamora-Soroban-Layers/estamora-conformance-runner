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

## What this build does with that command

```console
$ estamora run --profile sep-41@1.0 --contract CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC --network testnet
CONTRACT_RESOLUTION_ERROR: contract CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC on
`testnet` cannot be resolved by this build: reading a deployed contract's interface and state needs
a Soroban RPC transport, and none is linked into this runner. No verdict about the contract was
reached, and this failure is classified as an environment failure rather than as a conformance result
  contract: CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC
  network: testnet
  reason: network-transport-unavailable
$ echo $?
4
```

That is the honest answer, and it is deliberately not a graceful one. A runner
that accepted a contract identifier, did nothing with it, and printed a verdict
would be the worst possible outcome here: a fabricated `CONFORMANT` is
indistinguishable from a real one to anyone who did not run it. So the target
resolution layer is written as a boundary — the CLI, the engine and the report all
handle a remote target, and the one thing that is absent is the RPC transport —
and it fails with a named reason code instead of a generic message.

`integration-tests/testnet/` pins exactly this, including the property that
matters most:

```console
$ cargo test -p estamora-integration-tests --test testnet
```

Two of those tests assert that a network failure is classified as an environment
failure, exits `4`, and can never share an exit code with a non-conformant
contract. `PARTIALLY_CONFORMANT` and an unreachable node have nothing in common,
and a CI job that conflated them would train its users to ignore it.

## What a network run would do, and why the shape is already fixed

Reading a deployed contract needs two things the local path also needs, in the
same order:

1. **Inspection** — the contract's interface, read from its on-chain contract
   metadata. This is one conformance layer, not a verdict: a contract can expose
   every method and still behave incorrectly, which is the reason Estamora exists.
2. **Invocation** — the vectors, executed against the deployed contract.

Neither is a stub. `estamora inspect` reads an interface from a compiled artefact
today, and the same inspection model is what a remote resolution would populate. So
the difference between a local run and a network run is one transport, not a second
code path, and the report shape is already the one a network run would produce.

## Networks without a live node

For the parts of the pipeline that must be exercised deterministically, the local
ledger *is* the abstraction: `fixtures/contracts/` are compiled to real Wasm and
run in a real Soroban host, with the ledger sequence, the timestamp and every
account's authorization fixed by the vector. That is not a mock of a network — it
is the same execution environment, with time and randomness pinned — and it is why
a stored report can be re-read and produce the same verdict a year later.

Testnet execution is therefore opt-in and separate from the default suite. It is
never a reason for the ordinary `cargo test` to require a network, and `scripts/`
carries the runner for it separately.
