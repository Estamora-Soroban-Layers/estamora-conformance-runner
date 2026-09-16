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

## A measurement that was actually made

[`report.json`](report.json) beside this file is the output of a real run, not an
illustration. It is committed because the question a reader of this repository
should be able to answer without taking anything on trust is whether the `.wasm`
target has ever met a contract over a network, and a document produced by the
tool is a better answer than a paragraph saying it works.

| | |
| --- | --- |
| contract | `CDB3EKMUGN5E7X2LMO56IB3A55EU4PPUYEF5EBVDKDV3LCLICJNSYKLW` |
| network | `testnet` |
| artifact | `sha256:8393f410…07ffc688` — the digest of `fixtures/wasm/measurable-token.wasm` |
| profile | `sep-41@1.0`, digest `sha256:94654291…e732aee6` |
| runner | `estamora 0.1.3`, `2026-09-16T08:21:58Z` |
| reported | 63 checks, 0 failed · 1 vector passed, 19 skipped |
| verdict | `INCONCLUSIVE` (exit `2`) |

The deployment recorded above is the **second** one. The first was
`CBOBLQVLTYGMB3JDILHJFL5EMUWXIAKUW2N3RTOW2NKYOHC45CPSFV2P`, at artifact digest
`sha256:0fb7bc3d…b1eedc48`, and it was superseded when `burn` and `burn_from` were
given the negative-amount refusal they were missing — before that, `burn` with a
negative amount credited the holder rather than failing. The artifact was rebuilt from
the corrected source, redeployed, and re-measured, so the digest and the contract above
name the same bytes and the verdict below belongs to a contract that refuses a negative
amount. Anyone citing the earlier pair is citing the fix's own motivating example.

The artifact resolves over RPC, the instance's own code hash is compared against
the fetched bytes, the interface is read out of the deployed artifact's spec
section, and every check the profile declares held: 49 interface, 4
authorization, 6 event, 2 behavioural, 1 state and 1 invariant. One of the twenty
vectors — `unknown-account-balance-is-zero` — needs no seeded state, so it was
decided against the contract over the network and passed.

### Why the verdict is not `CONFORMANT`

The other nineteen vectors declare an opening balance or allowance, and a vector's
world cannot be established in a deployed artifact: this runner does not write to
the ledger it reads from, and the fixture entry points it uses to seed a local
fixture are not part of any standard. A requirement that was never exercised is
not a requirement that held, so the run is `INCONCLUSIVE` rather than conformant —
and it is deliberately not `NON_CONFORMANT` either, because nothing about the
contract is being blamed for the runner's inability to set up a scenario.

That is the honest shape of a remote measurement today, and it is why the profile
splits its corpus: the vectors that need a seeded world are the ones that must be
measured where state can be written, and the ones that do not are measurable
anywhere.

### Reproducing it

```bash
# The same artifact, built from the same source under the pinned toolchain.
./scripts/build-fixture-wasm.sh

# Deploy it. Any funded testnet account will do.
stellar contract deploy \
    --wasm fixtures/wasm/measurable-token.wasm \
    --source <identity> --network testnet

# Measure what was deployed.
estamora run --profile sep-41@1.0 --contract <contract-id> --network testnet \
    --report report.json
```

A different deployment produces a different contract identifier and a different
timestamp, so the report will differ in those fields. What it must not differ in
is the artifact digest: that field is what ties a verdict to a compilation.

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
