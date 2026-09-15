# What a non-conformant verdict looks like

The situation: a contract is wrong in exactly one way, and you want to see how that
is reported.

The runner ships seven contracts that are wrong on purpose. Each is the reference
token with one defect injected, and each is named after the defect so that a
failing test says what broke:

| Fixture | The defect | What it is for |
| --- | --- | --- |
| `fixture:none` | none | The control. Anything that fails against it is the runner's bug, not the contract's. |
| `fixture:skips-authorization` | moves value without consulting authorization | The defect a positive-only suite cannot see. |
| `fixture:omits-event` | settles without emitting the transfer event | Correct state, wrong ledger for every indexer downstream. |
| `fixture:double-emits` | emits the transfer event twice | A consumer that counts events double-counts. |
| `fixture:wrong-event-amount` | emits an amount that disagrees with the balance change | The state is right and the announcement is a lie. |
| `fixture:wrong-credit-amount` | debits one amount and credits another | Value lost in transit, with both calls "succeeding". |
| `fixture:allows-overdraft` | permits a transfer above the balance | Produces a balance the interface cannot represent. |
| `fixture:missing-decimals` | no `decimals` method at all | The interface defect, and the most instructive one. |

They are real contract code, written against the same SDK a user's contract uses, and
executed in a real Soroban host with time and authorization pinned. They are registered
from their Rust types rather than deployed from WebAssembly, which is how a fixture can
exist at all without a build step — see `docs/local-testing.md`. None of them is a mock,
and none of them is a contract written to fail a specific assertion: each is a mistake a
real token has made.

## Run one

```console
$ estamora run --profile fixtures/profiles/conformance-token/1.0 --contract fixture:skips-authorization
✓ balance-is-reported (passed)
✓ transfer-beyond-the-balance-fails (passed)
✗ transfer-moves-the-exact-amount (failed)
    [authorization] ✗ check: authorization/principal/sender-authorizes-transfer
        expected: alice
        observed: <no authorization required>
    [authorization] ✗ check: authorization/coverage/sender-authorizes-transfer
        expected: exactly(from)
        observed: <no arguments>
✗ transfer-without-authorization-fails (failed)
    [authorization] ✗ check: authorization/plan/transfer-without-authorization-fails
        expected: the call is refused
        observed: accepted
    … four more authorization failures, and one each in event, state,
      invariant and failure …
────────────────────────────────────────────────────────────────────────
checks by dimension
  interface      10 passed, 0 failed
  authorization   6 passed, 4 failed
  event           9 passed, 1 failed
  behavior        7 passed, 0 failed
  state           5 passed, 2 failed
  invariant       3 passed, 2 failed
  failure         5 passed, 1 failed
  total          55 reported, 10 failed
────────────────────────────────────────────────────────────────────────
status PARTIALLY_CONFORMANT (exit 1)
```

Read the failure spread rather than the status. One defect — authorization never
consulted — fails checks in five dimensions, because the consequences of a missing
authorization check are a value that moved, an event that should not have been
emitted, a balance that changed when it must not, an invariant that broke and a
refusal that did not happen. A runner that reported "authorization: failed" and
stopped would have hidden four of them.

Ten failed checks across two of four vectors, and `PARTIALLY_CONFORMANT` rather
than `NON_CONFORMANT` — because two vectors did pass. The other two are the ones
that speak about moves of value, and they are exactly the two the defect touches.

Pass `--verbose` to list every check that was made, including the ones that held.
The tally separates `reported` from `failed` precisely because a check the vector
did not exercise must not be counted as a check that held.

## The one that passes

```console
$ estamora run --profile fixtures/profiles/conformance-token/1.0 --contract fixture:missing-decimals
status CONFORMANT (exit 0)
```

`fixture:missing-decimals` is conformant here — and that is the correct result.
The fixture profile is a *partial* profile: it declares two methods, `transfer`
and `balance`, and says nothing about `decimals`. An undeclared method is not an
unrequired one; it is outside what this profile measures. A runner that reported a
failure here would be inventing a requirement, which is the other half of the
mistake this project exists to prevent.

Measure the same contract against a profile that does declare `decimals`:

```console
$ ESTAMORA_SPEC_REPO=/path/to/estamora-conformance-spec \
  estamora run --profile sep-41@1.0 --contract fixture:missing-decimals
    [interface] ✗ check: interface/method/decimals
        expected: required method decimals is available
        observed: absent from the fixture `missing-decimals`
  interface      46 passed, 1 failed
  total          300 reported, 4 failed
status NON_CONFORMANT (exit 1)
```

Same contract, same code, two different and both correct verdicts. The profile is
what defines conformance, and this pair of runs is the clearest demonstration of
what that means. It is also why a report names the profile and its digest: a
verdict without them is not interpretable.

## The difference between the two exit codes

Both runs above exit `1`, and they mean different things.

`PARTIALLY_CONFORMANT` is some vectors passing and some failing — the ordinary
result of measuring a real contract against a profile that spans more of an
interface than it has finished. `NON_CONFORMANT` is a requirement being
contradicted outright. Neither is "the run broke": a run that broke exits `4` and
is not a statement about the contract at all.

## Failing tests

Every one of these fixtures has a test asserting the dimension it fails, so the
classification cannot silently change:

```console
$ cargo test -p estamora-integration-tests
```
