# Troubleshooting

The error class is the diagnosis. Every failure names one, the classes never share
an exit code, and the message carries the specifics as `key: value` context. Read
the class first and the message second; the message is written to be quoted into a
bug report as-is.

| Class | Exit | Whose problem |
| --- | --- | --- |
| `PROFILE_ERROR` | `3` | The profile. |
| `VECTOR_ERROR` | `3` | The corpus. |
| `CONTRACT_RESOLUTION_ERROR` | `4` | The environment — the artifact or contract could not be reached. |
| `NETWORK_ERROR` | `4` | The environment. |
| `EXECUTION_ERROR` | `4` | The environment — the vector's world could not be built. |
| `ASSERTION_FAILURE` | `1` | The contract. |
| `REPORT_ERROR` | `5` | The document you handed the runner. |
| `CERTIFICATION_ERROR` | `5` | The receipt, key or signature. |
| `USAGE_ERROR` | `64` | The command line. |
| `INTERNAL_ERROR` | `5` | Estamora. Please report it. |

Only `ASSERTION_FAILURE` is a statement about a contract. If you see `4`, the
contract was not measured and nothing about it has been established.

## The profile could not be found

```text
PROFILE_ERROR: "nope@9.9" does not name a profile bundle: it is not a directory
holding a profile.yaml and nothing was found at ../estamora-conformance-spec/profiles/nope/9.9
  profiles: ../estamora-conformance-spec/profiles
```

The message names the directory it looked in, which is the fastest way to see which
checkout it actually read. `--spec` defaults to `$ESTAMORA_SPEC_REPO`, then to the
sibling directory. Set one of them, or pass a path to the bundle directly.

`id@version` resolves under `<spec>/profiles/<id>/<version>/`. A path is also
accepted, and a directory containing a `profile.yaml` is the third spelling, which
is how a bundle under development is measured before it is contributed.

## A document in the bundle could not be read

```text
PROFILE_ERROR: the bundle declares a methods document at "methods.yaml" and it could
not be read: No such file or directory (os error 2)
  bundle: fixtures/malformed-inputs/profile-missing-document
  document: methods.yaml
```

The bundle's manifest lists every document it needs, and **every one of them is
required, including the empty ones**. A profile that declares no failure mode writes
`failures: []`; it does not omit the file. Tolerating an absent document would
silently drop a whole dimension, and a dropped requirement is indistinguishable from
one that was never made.

If you are authoring a bundle and see this, check the `includes:` block — a
mismatch between what it names and what is on disk is the usual cause.

## A cross-reference does not resolve

```text
PROFILE_ERROR: method "transfer" refers to behaviour "transfer-moves-exact-amount",
which the profile does not declare
```

An error and never a warning. A typo in a reference list would otherwise produce a
requirement that is declared, appears in `estamora profile` output, and is never
evaluated against anything — a check that silently does not happen.

`estamora validate` reports *all* of them at once rather than the first, so run it
after editing a bundle rather than fixing them one at a time. It also exits `3`, so
it works as a pre-commit or CI check.

## The format version is not supported

```text
PROFILE_ERROR: the bundle declares estamora-spec 2.0 and this runner implements 1.0
```

`estamora_spec_version` is the version of the document *format*, independent of the
profile version and of the runner version. Reading a bundle with the wrong rules
would be worse than not reading it, so the runner refuses by name. Update the runner,
or take the bundle from a revision that targets the format you have.

## The corpus does not agree with the profile

```text
VECTOR_ERROR: 6 defect(s) found; first: vector "balance-is-reported-exactly" refers
to the event "transfer", which the profile does not declare
```

A `VECTOR_ERROR` means the requirements are readable but the measuring document is
not usable. Common causes:

* a vector exercises a method the profile does not declare;
* a vector requires or forbids an event, invariant or failure the profile does not
  declare — for a forbidden event this usually means the profile needs to declare it
  with `requirement: forbidden`;
* a vector expects authorization to be unnecessary while listing signers;
* a vector declares no expected outcome.

A corpus that refers to something undeclared is refused rather than executed
partially, because executing it partially would produce a verdict over a corpus
smaller than the profile describes and nothing in the report would show it.

## The artifact could not be reached

```text
CONTRACT_RESOLUTION_ERROR: /nowhere/x.wasm is not a file, so there is no artifact to deploy
  contract: /nowhere/x.wasm
```

or

```text
CONTRACT_RESOLUTION_ERROR: no contract instance is stored at CDLZ…GCYSC on `testnet`. The
identifier is well formed, so this is a contract that does not exist on that network rather
than one that could not be read
  contract: CDLZ…GCYSC
  network: testnet
  reason: contract-not-found
```

The second means the contract is not on that network — a mistyped identifier, or one
from a different network. It exits `4`, not `1`: nothing about any contract's behaviour
was observed. `docs/testnet-testing.md` covers the rest of the resolution failures.

A contract that is loaded but publishes no `contractspecv0` section is also a
resolution error, because a contract with no declared interface cannot be measured
against a profile's interface requirements at all. Build for `wasm32v1-none` with
the Soroban SDK; a host build has no spec section and is not a contract artifact.

## An artifact is refused for its size

```text
CONTRACT_RESOLUTION_ERROR: ./huge.wasm is 200000000 bytes, above the 67108864-byte limit
```

The limit exists so that an oversized file is a resolution error rather than an
allocation. A Soroban contract is orders of magnitude smaller than this; if you hit
it, you are probably pointing at the wrong file.

## A `.wasm` declares a constructor that takes arguments

```text
CONTRACT_RESOLUTION_ERROR: the artifact ./my_token.wasm declares a constructor taking
admin: address, decimal: u32, and a constructor can only be run by whoever deployed the
contract: only the deployer knew what to pass it. … No instance can be created locally,
so nothing about this contract's behaviour was observed
  reason: constructor-needs-arguments
```

Registering an artifact runs its `__constructor`, and the environment has no arguments
to give it. The runner refuses rather than inventing them, because a contract placed in
a state the runner made up is measured against that invention and not against the
contract anybody deployed. Exit `4`: the environment is at fault, not the contract.

It applies to the `.wasm` path only. Measuring the same contract by identifier —
`--contract <id> --network <network>` — places it in the ledger from its deployed
instance entry and never runs its constructor, whatever the constructor took.
`docs/local-testing.md` and `docs/testnet-testing.md` have the two halves.

## A vector was skipped

```text
• transfer-moves-exact-amount (skipped)
    · seeding-unavailable the vector declares an opening balance or allowance and a
      compiled artifact is not assumed to publish Estamora's fixture setup entry points…
```

Not an error, not a pass, and the run's verdict becomes `INCONCLUSIVE` (exit `2`).
The vector was not exercised, and a requirement that was not exercised cannot be
reported as satisfied.

Against a fixture this does not happen — the fixtures publish setup entry points.
Against a `.wasm` it does, for every vector whose world declares an opening balance.
`docs/local-testing.md` explains why, and why the fix belongs in the specification
rather than in this runner.

## The world could not be built

```text
EXECUTION_ERROR: the opening state for ... could not be established: the contract
refused `fixture_mint`
```

A setup call the contract refused means nothing about conformance follows — the
contract was never measured on any requirement. The run stops with an environment
failure rather than marking every vector non-conformant, because the second outcome
would blame the contract for the runner's inability to prepare a scenario.

## A document you handed the runner is not what it claims

```text
REPORT_ERROR: the document is not a conformance report: unknown variant
`PROBABLY_FINE`, expected one of `CONFORMANT`, `PARTIALLY_CONFORMANT`,
`NON_CONFORMANT`, `INCONCLUSIVE`, `EXECUTION_ERROR`, `PROFILE_ERROR` at line 3 column 27
```

The parser is strict and the message includes the position. A rendering of a
document that was only partly understood would look exactly like a rendering of a
result, so nothing is rendered.

Note the exit code is `5`, not `1`. A malformed report must never share an exit code
with a non-conformant contract, or a broken pipeline would read as a broken contract
— `integration-tests/` asserts that it does not.

## A receipt does not verify

```text
CERTIFICATION_ERROR: the signature does not check out under the supplied key
```

or

```text
attributed to nobody: no trusted key was supplied, so the signature was not checked…
```

The second is not a failure. Without `--public-key`, verification still checks that
the receipt commits to the report it was handed and reports that no issuer is
identified. Supplying the key is what turns *the report matches* into *and this key
asserted it*. `docs/certification.md` explains why a key carried inside the receipt
is not evidence of authorship.

## A run passed but you expected a failure

Check which profile was used and what it declares:

```console
$ estamora profile --profile <the-one-you-used>
```

A profile states what is measured, and an interface it does not mention is not
thereby required. `fixture:missing-decimals` is *conformant* against a profile that
declares only `transfer` and `balance`, and *non-conformant* against SEP-41, and both
results are correct. The profile is the definition of conformance; a report names it
and its digest for exactly this reason.

`estamora run --verbose` lists every check that was made and every check that was
reported as inapplicable, which is the fastest way to see what a run did *not*
establish.

## Nothing seems to work and the messages are all `4`

The environment is failing, not the contract. Check, in order: the artifact path
exists and is a Soroban build for the right target; the specification checkout is
present and at the revision you meant; the ledger timestamp in the vector is a valid
RFC 3339 instant. Nothing in that list is about the contract, which is the point of
the classification.
