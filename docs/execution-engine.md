# The execution engine

This is the detailed version of the pipeline in `docs/architecture.md`: what the
engine does for one vector, what it observes, and how a requirement becomes a
check that either holds or does not.

## What is observed

A vector declares a world, an operation and an expected outcome. The engine turns
the world into a real execution environment, performs the operation, and captures
six things:

| Observation | What it is |
| --- | --- |
| `outcome` | Whether the call returned or was refused, and if refused, under which failure category. |
| `returns` | The value the call returned, as a typed value. |
| `events` | Every event emitted by the contract during the call, in order, with topics and payload. |
| `authorization` | What the host records as having been authenticated: the address that signed and the argument values its signature covered. |
| `before` / `after` | The world the vector declared, before the call and after it. |
| `plan` | Which authorization path the vector set up — signed, unsigned, or a signature from the wrong actor. |

Nothing else is captured, and everything captured is in the report. An assertion
that wanted a fact outside this set could not be evaluated, which is the mechanism
that keeps a check from depending on something the report cannot show.

The `before` and `after` worlds matter more than they look. A postcondition such as
"Alice's balance decreased by the amount transferred" is not evaluated against a
literal; it is evaluated against the world before and the world after, so the same
rule holds for every starting balance rather than for the amounts one vector
happens to use.

## The seven dimensions

Each dimension evaluates a different kind of requirement, and each reports its own
checks. A run's tally is per dimension, because "the interface is fine and the
events are wrong" is actionable and "conformance: failed" is not.

### Interface

Evaluated once per run, against the contract's declared interface read from its
`contractspecv0` metadata: that each required method exists, that its arity matches,
that each parameter's type is compatible, and that its return type is compatible.

Each of those is a separate check. `interface/method/decimals` failing and
`interface/parameter/transfer/0` failing are different findings with different
remedies, and a single boolean would merge them.

An interface is the weakest claim Estamora makes. A contract that exposes every
method a profile declares can violate every behavioural requirement in it, which is
why a passing interface section never produces a verdict on its own.

### Authorization

Authorization is a first-class dimension, not a fifth assertion inside behaviour.

The engine establishes what the contract demanded by comparing the addresses the
host recorded as authenticated against the call's own argument values — so the
coverage it reports is what the contract *required*, not what the vector happened
to offer. It then checks the principal the profile names, the arguments the
profile says the signature must cover, and the outcome: that an authorized call was
accepted, and that an unauthorized or wrong-actor call was refused with the failure
category the profile names.

The dimension distinguishes four things that a naive suite collapses:

* an authorized call that succeeded — the expected path;
* an unauthorized call that was refused — also the expected path;
* an unauthorized call that **succeeded** — a conformance failure, and the one this
  dimension exists for;
* a call that could not be observed at all — a finding, not a pass.

That last one deserves its own paragraph. A contract that refuses a call unwinds the
host's record of what was authenticated; there is nothing left to compare the
profile's requirement against. The engine reports
`authorization-unobservable-…` as a **diagnostic** with the reason, rather than
claiming a check that held. A contract that never asks for authorization will be
caught by the unauthorized-succeeds case and by the outcome check; it does not need
a fabricated pass here, and a fabricated pass is exactly how a suite stops meaning
anything.

### Events

Every emitted event is captured in order. The dimension checks presence, absence,
cardinality, topic values, payload values, and ordering constraints.

Event values are **bound**, not hard-coded. A profile says that a transfer event's
amount must equal the amount that was actually transferred — via a binding to the
operation's input, or to a state value read after the call — rather than stating
`250`, so the requirement survives a change of fixture. A vector that repeated the
literal would pass a contract that emitted a stale constant.

Forbidden events are enforced in the same pass as required ones. A call that must
be refused and emits the success event anyway is the failure this dimension is most
valuable for, because the contract's own state is correct and nothing else notices.

Matching is on the profile's declared `name` and nothing else. Matching on a
source-language type name would make the requirement a test of one implementation
language.

### Behaviour

Preconditions, postconditions and outcome expectations, evaluated against the
before and after worlds rather than against literals.

A behavioural rule is scoped by the outcome the vector expects. The SEP-41 profile's
`transfer-moves-exact-amount` rule describes the path where the transfer succeeds;
applying it to a vector that expects the transfer to be refused would fail a
conforming contract. The engine therefore reports the rule as
`rule-inapplicable-…`, naming the reason, on every vector whose expected outcome
the rule does not describe. A rule that cannot apply is a finding, not a pass, and
it is visible in the terminal output by default precisely because it is the
difference between "this held" and "this was never tried".

### State

Exact values, unchanged values, relative changes, affected resources, and the
whole-state snapshot.

The snapshot comparison is why this dimension has its own `before`/`after` capture
rather than reusing a rule's preconditions: a contract can satisfy every named
assertion and still leave a ledger entry rewritten. The snapshot catches what
nothing named.

### Invariants

Reusable properties: balances conserved, total supply unchanged by a transfer, no
balance below zero, a failed operation cannot mutate protected state, an
unauthorized operation cannot mutate state.

An invariant is declared once and referenced by the vectors it applies to, and it
carries a scope — the methods and the outcomes it ranges over. An invariant outside
its scope is reported as inapplicable rather than satisfied, for the same reason a
behavioural rule is: `balances-are-non-negative` evaluated over an empty set of
accounts has not been verified, it has been skipped.

### Failure

The reason a call failed, as distinct from the fact that it failed.

Where a standard does not fix an error code, the profile states a **semantic
failure category** — insufficient balance, unauthorized caller, invalid amount —
and the dimension checks that the refusal was of that kind. The model separates
four outcomes that a suite must not merge:

| Outcome | Verdict |
| --- | --- |
| The call was refused, as the profile requires, and for the stated reason. | The vector holds. |
| The call was refused, and for a different reason. | Failed: the classification is wrong. |
| The call succeeded where the profile requires a refusal. | Failed: `UNEXPECTED_SUCCESS`. |
| The call could not be evaluated. | Neither: a finding for the environment. |

An expected failure is never recorded by testing merely that *something* went
wrong. A contract that returns an error because it panicked is not a contract that
refuses an overdraft, and a runner that accepted the panic would report a broken
contract as conformant.

## From checks to a verdict

The checks are not the verdict. The verdict is the **vector** results reduced by a
rule that lives in one place, in `estamora-core`:

```
  any required vector undecided?
      yes, and none decided  -> EXECUTION_ERROR   (the environment failed)
      yes                    -> INCONCLUSIVE      (the suite could not decide)
  any required vector failed?
      yes, and none passed   -> NON_CONFORMANT
      yes                    -> PARTIALLY_CONFORMANT
  no required vector passed  -> INCONCLUSIVE      (nothing was measured)
  otherwise                  -> CONFORMANT
```

Three consequences are deliberate.

**A vector that produced no verdict is not a pass.** `Error` and `Skipped` are
counted together as undecided, because both mean the same thing to a verdict: this
requirement was not exercised. Counting them as passes is how a suite that ran
nothing reports itself as clean.

**An optional vector never decides anything.** A failing optional vector is
recorded so the report can show it, and nothing more — a profile marks a
requirement optional precisely because a conforming implementation may not satisfy
it.

**`INCONCLUSIVE` is not on the contract's exit code.** It says the suite could not
decide, so a gate that treated it as a contract failure would be reporting a broken
runner as a broken contract. It exits `2`, separately from `1`.

`estamora-report` turns the same results into the normative JSON document,
Markdown for a reviewer and JUnit for an existing CI gate. All three are renderings
of one model, so a format cannot disagree with the verdict — and an undecidable
vector reaches JUnit as `error`, never as `failure`, for the same reason it exits
`2` rather than `1`.
