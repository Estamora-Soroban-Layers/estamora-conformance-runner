# Measuring a contract against SEP-41

The situation: you have a token contract and want to know whether it actually
implements SEP-41, the Soroban token interface.

This is the example that matters most, because SEP-41 is the profile Estamora does
not own. Its requirements are authored in `estamora-conformance-spec` by people who
are not the people who wrote this runner, and the runner has no special knowledge of
it: it loads a bundle, executes its vectors, and evaluates its rules. A defect that
made the runner agree with its own fixtures while disagreeing with the standard
would show up here and nowhere else.

## Run it

```console
$ export ESTAMORA_SPEC_REPO=/path/to/estamora-conformance-spec
$ estamora run --profile sep-41@1.0 --contract fixture:none
```

```text
✓ allowance-expired-reads-as-zero (passed)
✓ approve-overwrites-allowance (passed)
✓ balance-of-unknown-address-is-zero (passed)
✓ burn-reduces-holder-balance (passed)
✓ burn-from-beyond-allowance-fails (passed)
✓ decimals-reports-configured-precision (passed)
✓ name-reports-non-empty-name (passed)
✓ symbol-reports-non-empty-symbol (passed)
✓ transfer-emits-exactly-one-event (passed)
✓ transfer-moves-exact-amount (passed)
✓ transfer-from-beyond-allowance-fails (passed)
✓ transfer-from-wrong-actor-fails (passed)
✓ unknown-account-balance-is-zero (passed)
✓ zero-amount-transfer-succeeds (passed)
✓ transfer-without-signature-fails (passed)
✓ transfer-beyond-balance-fails (passed)
✓ approve-grants-allowance (passed)
✓ funded-balance-is-reported (passed)
✓ transfer-authorized-by-wrong-actor-fails (passed)
✓ transfer-from-consumes-allowance (passed)
────────────────────────────────────────────────────────────────────────
checks by dimension
  interface      49 passed, 0 failed
  authorization  34 passed, 0 failed
  event          102 passed, 0 failed
  behavior       43 passed, 0 failed
  state          26 passed, 0 failed
  invariant      15 passed, 0 failed
  failure        33 passed, 0 failed
  total          302 reported, 0 failed
────────────────────────────────────────────────────────────────────────
status CONFORMANT (exit 0)
```

(The `·` notes that the run prints under several vectors are omitted above for
width; `docs/execution-engine.md` explains what they are.)

Twenty vectors, three hundred and two checks. The count matters: the corpus is not
twenty separate questions, it is a few hundred, because every vector is measured
against every requirement the profile declares and every dimension reports
individually. A single boolean would have thrown most of that away.

Those counts are not stable across profile revisions, and they are not meant to be.
A run that reported the same number after SEP-41 gained a requirement would have
ignored it.

## Where the vectors come from

Note the last line of `estamora profile --profile sep-41@1.0`: the corpus is a
union. `profiles/sep-41/1.0/vectors/` holds the vectors that need SEP-41's own
requirements, and `vectors/` holds the shared ones — a transfer beyond the
balance fails, an unknown account's balance is zero — which are stated once and
included by every profile that needs them. A profile that had to restate them
would drift from the others, and the drift would be invisible.

```console
$ estamora profile --profile sep-41@1.0
```

## The interesting failure

`fixture:missing-decimals` is the reference contract with its `decimals` method
removed, and it is the case that separates interface checking from conformance:

```console
$ estamora run --profile sep-41@1.0 --contract fixture:missing-decimals
✗ allowance-expired-reads-as-zero (failed)
    [interface] ✗ check: interface/method/decimals
        expected: required method decimals is available
        observed: absent from the fixture `missing-decimals`
✗ approve-overwrites-allowance (failed)
    [interface] ✗ check: interface/method/decimals
        expected: required method decimals is available
        observed: absent from the fixture `missing-decimals`
… sixteen more vectors, each failing the same interface check …
✗ decimals-reports-configured-precision (failed)
    [interface] ✗ check: interface/method/decimals
        expected: required method decimals is available
        observed: absent from the fixture `missing-decimals`
    [behavior] ✗ check: behavior/metadata-reads-do-not-mutate-state/outcome/decimals-reports-configured-precision
        expected: accepted
        observed: refused
    [authorization] ✗ check: authorization/plan/decimals-reports-configured-precision
        expected: the call is accepted with no signature
        observed: refused
    [authorization] ✗ check: authorization/outcome/reads-require-no-authorization/unauthorized
        expected: accepted
        observed: refused
────────────────────────────────────────────────────────────────────────
checks by dimension
  interface      46 passed, 1 failed
  authorization  32 passed, 2 failed
  event          102 passed, 0 failed
  behavior       42 passed, 1 failed
  state          26 passed, 0 failed
  invariant      15 passed, 0 failed
  failure        33 passed, 0 failed
  total          300 reported, 4 failed
────────────────────────────────────────────────────────────────────────
status NON_CONFORMANT (exit 1)
```

Two things are worth reading carefully here.

**Every vector is reported as failed, and the tally counts four checks.** The
interface is a precondition of everything else, so a contract that does not expose
a required method has not been measured by any vector, and calling them anything
other than failed would be claiming coverage that does not exist. The tally counts
*distinct checks that failed*, and `interface/method/decimals` is one check that
appears in all twenty vectors: it is counted once. Four distinct checks failed;
twenty vectors were unmeasurable. Both numbers are in the output because both are
useful, and neither substitutes for the other.

**The missing method does not stop the run.** The other 296 checks were still made
and they held, which is what makes the report actionable: the contract's transfer
path is fine and its metadata path is not, and a runner that aborted at the first
interface defect would have told you neither.

## Measuring something you built

`fixture:none` is a stand-in. The real invocation replaces `--contract` with your
own artefact:

```console
$ estamora run --profile sep-41@1.0 --contract ./my-token.wasm --report report.json
```

The `.wasm` must be the contract target build, not an ordinary host build: a
Soroban contract is compiled for `wasm32v1-none` (or `wasm32-unknown-unknown` on
older toolchains), and a host build will not load. See `docs/local-testing.md`.

## What a passing run means, and what it does not

It means: the contract's deployed code satisfied the SEP-41 profile at version
1.0, over the named corpus, for every recorded check. Which profile, at which
revision, is in the report, because a requirement that changed between revisions
changed what "conformant" meant.

It does not mean the contract is secure, that it is free of logic errors outside
what SEP-41 describes, or that its issuer's accounting is honest. SEP-41 says
what the token interface must do; it says nothing about how supply is minted, who
is allowed to mint it, or whether the admin key is safe. `docs/security.md`.
