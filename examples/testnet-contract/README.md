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
| contract | `CDMCJRW5QBTOOOGYDPCJV6N4RKLX44V6XWKNN6ZFAKN6J2F5HQPSNAOV` |
| network | `testnet` |
| artifact | `sha256:5cbcc4ab…9f8707cc` — the digest of `fixtures/wasm/measurable-token.wasm` |
| profile | `sep-41@1.0`, digest `sha256:94654291…e732aee6` |
| runner | `estamora 0.1.3`, `2026-09-16T15:51:23Z` |
| reported | 63 checks, 0 failed · 1 vector passed, 19 skipped |
| verdict | `INCONCLUSIVE` (exit `2`) |

The deployment recorded above is the **third** one, and the pairing is current: the digest in
the table is the digest of the artifact this repository commits, which is the property the
history below is about.

The first deployment, `CBOBLQVLTYGMB3JDILHJFL5EMUWXIAKUW2N3RTOW2NKYOHC45CPSFV2P` at
`sha256:0fb7bc3d…b1eedc48`, was superseded when `burn` and `burn_from` were given the
negative-amount refusal they were missing — before that, `burn` with a negative amount credited
the holder rather than failing. Anyone citing that pair is citing the fix's own motivating
example.

The second, `CDB3EKMUGN5E7X2LMO56IB3A55EU4PPUYEF5EBVDKDV3LCLICJNSYKLW` at
`sha256:8393f410…07ffc688`, was superseded by this one. Its source built each storage key twice
where it needed it once; the artifact and the deployment recorded above come from a source that
builds it once, which leaves the compiled contract **115 bytes smaller**, 11,324 to 11,209. The
per-call cost did not move — the measurements in [What these calls cost](#what-these-calls-cost)
differ by a few dozen instructions in either direction, which is ledger state rather than code —
because what a call pays for is the storage reads and writes, and what a deployment pays for is
the size of the code that performs them.

Pairing the digest with the contract is not left to a reader to check.
`the_committed_network_measurement_reads_re_renders_and_agrees_with_the_artifact` fails if the
report's code hash is not the hash of the artifact in this tree, which is why the artifact and
the deployment are always rebuilt and redeployed together rather than one at a time.

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
# The artifact as it stands in this tree, under the pinned toolchain.
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
timestamp, so the report will differ in those fields. What it must not differ in is the
artifact digest: that field is what ties a verdict to a compilation, and the test named above
is what keeps it the digest of the artifact in this tree.

## What these calls cost

A conformance verdict says whether a contract behaves as a profile requires. It says nothing
about what calling it costs, and a token author asking "what does my token cost?" is asking a
different question. This is the answer for the example token, measured rather than estimated.

| entrypoint | call | CPU instructions | ledger bytes | resource fee |
| --- | --- | ---: | ---: | ---: |
| `decimals` | a constant, read | 429,437 | 0 | 12,648 stroops |
| `name` | a constant, read | 433,461 | 0 | 12,722 stroops |
| `symbol` | a constant, read | 433,808 | 0 | 12,672 stroops |
| `balance` | a balance that does not exist | 478,900 | 0 | 13,446 stroops |
| `allowance` | an allowance that does not exist | 490,890 | 0 | 13,872 stroops |
| `approve` | revoking an allowance with zero | 519,692 | 216 written, 0 read | 20,387 stroops |
| `burn` | burning nothing | 519,424 | 148 written, 0 read | 39,154 stroops |
| `burn_from` | burning nothing against a zero allowance | 623,241 | 364 written, 0 read | 46,089 stroops |
| `transfer` | refused: the holder has a zero balance | - | - | - |
| `burn` | refused: a negative amount | - | - | - |
| `approve` | refused: a negative amount | - | - | - |

Measured against `CDMCJRW5…` on testnet at ledger 4710047, by `scripts/measure-contract-costs.py`. Instructions and bytes are what Soroban charges for; the resource fee is their price in stroops, taken from the simulation's `minResourceFee` and excluding the inclusion fee and any refund of unused bytes.

A refused call is listed because its refusal is part of the contract's surface, and it has no numbers against it because a failed simulation returns no resource data: the resources consumed are carried inside the transaction data a successful simulation produces, and are absent here rather than zero.

### How it was measured

`simulateTransaction` against testnet, for the deployment named above. The transaction for each call is
built with `stellar contract invoke --build-only`, which needs a source account and no signature, and
the simulation is what reports the resources: the instructions the host charged, the ledger bytes the
call would write and read, and `minResourceFee`, the stroop price of those resources.

Nothing is signed and nothing is submitted, which is why a public key is enough. Simulation records
the authorization a call requires rather than verifying a signature, so the calls below are the ones a
caller with no balance and no allowance can legitimately make: the read paths, revoking an allowance
with zero, and burning nothing. The rows with no numbers are calls the contract **refuses**, and they
carry no figures because a failed simulation returns no resource data at all. They are listed rather
than omitted so that the table shows the contract's refusals instead of measuring around them.

Regenerate it with:

```bash
./scripts/measure-contract-costs.py measure > examples/testnet-contract/costs.json
./scripts/measure-contract-costs.py render examples/testnet-contract/costs.json
```

`ci.yml` runs `measure-contract-costs.py check`, so this table and the capture it came from cannot
drift apart. The check is offline and does not re-measure: a fee that is re-measured is a fee that
changes between runs, and a document that changes between runs is not a record of anything.

### What the numbers say

Read the table for its shape rather than its last digit. **Every call costs between roughly 430,000
and 625,000 instructions, and the cheapest call is the one that does least: `decimals` returns a
compiled constant.** A contract invocation has a fixed cost before any of a contract's own code runs
— host setup, the footprint, decoding the arguments — and that floor is most of every figure here.
The token's own logic sits on top of it: a storage read that misses, a temporary allowance written,
a persistent balance written.

That has a consequence worth stating plainly for anyone optimising a token for fees: **arithmetic
inside a token method is not where the fee is.** The spread across this table comes almost entirely
from how many ledger entries a call touches — `burn_from`, which reads and writes both an allowance
and a balance, is the most expensive call measured, at 364 bytes written against 216 for a bare
`approve`. Reducing the number of ledger entries a call touches is the only lever here that moves
the fee by more than a rounding error, and no call path in this contract touches an entry it does
not need.

Two qualifications on the fees, because they are the numbers most likely to be quoted. They are
`minResourceFee` in stroops, so a read is about 0.0013 XLM and `burn_from` about 0.0045 XLM. They
**exclude** the inclusion fee and exclude the refund of unused bytes the network returns when a
transaction is submitted, so a simulated fee is an upper bound on the resource cost rather than the
amount finally charged.
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
