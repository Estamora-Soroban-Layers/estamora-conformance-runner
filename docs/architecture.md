# Architecture

Estamora is two repositories, and the boundary between them is the most important
thing about its architecture:

| Repository | Owns |
| --- | --- |
| [`estamora-conformance-spec`](https://github.com/Estamora-Soroban-Layers/estamora-conformance-spec) | What it means for a contract to conform: profiles, schemas, vectors, and the normative semantics of a result. |
| `estamora-conformance-runner` (this one) | Executing those profiles against a real contract and reporting what happened. |

This repository contains **no normative claims**. It does not decide what SEP-41
requires, what a failure category means, or what a report must contain; it reads
those from the specification and applies them. When the two disagree, the
specification is right and this repository has the defect. That is why there is no
`profiles/` directory here with a vendored copy of a standard — a second copy is a
second authority, and it would drift.

## The crate layers

The workspace is layered so that each crate depends only on the layers below it.
Nothing below `estamora-soroban` may depend on it, and that rule is the one that
keeps the specification-consuming and reporting layers testable without a network
or an execution environment.

```
                estamora-cli                     the `estamora` binary
                     |
      +--------------+--------------+-----------+
      |              |              |           |
 estamora-         estamora-    estamora-   estamora-
 assertions        report       certification
      |              |
      |              |
 estamora-soroban ---+
      |
 estamora-vectors --+
      |
 estamora-profile --+
      |
  estamora-core      error taxonomy, outcomes, the verdict rule, expression algebra
```

| Crate | Responsibility |
| --- | --- |
| `estamora-core` | The vocabulary every other crate speaks: `ErrorClass`, `ConformanceStatus`, `VectorStatus`, `ExitCode`, the verdict rule, the expression algebra, and the observed-value model. Depends on nothing but `serde` and a digest. |
| `estamora-profile` | Loads a profile bundle from the specification repository, validates it against the published JSON Schemas, and resolves every cross-reference between its documents. |
| `estamora-vectors` | Loads the vector corpus, resolves each vector against the profile it names, and refuses one that refers to something the profile does not declare. |
| `estamora-soroban` | The only crate that knows Soroban exists: contract resolution, deployment, seeding, invocation, event and authorization capture, and interface inspection. |
| `estamora-assertions` | Evaluates a profile requirement against an observation, one dimension at a time. Pure: it takes an observation and returns outcomes, and touches no environment. |
| `estamora-report` | The report document and its renderings — the normative JSON, Markdown, and JUnit. |
| `estamora-certification` | Digests, receipts, signing and verification. |
| `estamora-cli` | The pipeline that assembles a run from the above, and the command surface. |

Two design consequences are worth stating explicitly.

**The assertion layer is pure.** `estamora-assertions` never opens a file, deploys
a contract, or reads a clock. It is handed an observation — what was called, what
was returned, what events were emitted, what authorization was recorded, what the
world looked like before and after — and it returns outcomes. Everything that
could make a check non-deterministic lives in `estamora-soroban`, and everything
that decides a verdict lives above both. A test for a rule therefore requires no
contract, which is why there are hundreds of them.

**Observation is a data structure, not a callback.** There is no way for an
assertion to "look something up" while it is being evaluated, because a lookup
would be an unrecorded side effect and the report would no longer be a complete
account of what was checked. What was observed is captured first, in full, and then
measured. It is also why `estamora-report` can re-render a stored run months later
and produce the same document.

## What a run does

```
  CLI arguments
        |
   resolve the profile bundle        -> PROFILE_ERROR if it is unusable
        |
   load and validate the corpus      -> VECTOR_ERROR if a vector is unusable
        |
   resolve the target contract       -> CONTRACT_RESOLUTION_ERROR if unreachable
        |
   inspect the interface             -> one dimension, not a verdict
        |
   for each vector:
        |  place the contract in a fresh host: registered from its bytes, or
        |  held from the instance entry a network reports for it
        |  seed the world the vector declares
        |  take the before-world
        |  invoke the operation with the declared authorization
        |  capture the return value, the emitted events, the recorded
        |  authorization and the after-world
        |  evaluate all seven dimensions against the profile
        |
   reduce the vector results to a status by the verdict rule
        |
   render the report; optionally issue a receipt
        |
   exit with one of seven codes
```

The per-vector loop builds a **fresh host**, and which host depends on the target. A
target on disk or in the repository is deployed into it; a target on a network *is* it,
because a deployed contract is placed in a ledger assembled from its own instance entry
rather than redeployed. Either way a run is not a sequence of operations on one contract
instance, because a vector's declared starting state is a claim about the world the
operation sees, and inheriting a previous vector's mutations would make the corpus
order-dependent. Order-dependence is how a suite starts passing for the wrong reason.

`docs/execution-engine.md` goes through the loop in detail, including what happens
when a contract call is refused and why the authorization dimension then reports a
finding rather than a pass.

## Where a failure is attributed

The runner separates six questions that are easy to collapse into one, and the
separation is what makes its output usable by a pipeline:

| Question | If the answer is no |
| --- | --- |
| Are the requirements usable? | `PROFILE_ERROR` / `VECTOR_ERROR` — exit `3` |
| Can the contract be reached? | `CONTRACT_RESOLUTION_ERROR` / `NETWORK_ERROR` — exit `4` |
| Can the vector's world be built? | `EXECUTION_ERROR` — exit `4` |
| Did the contract violate a requirement? | an assertion failure — exit `1` |
| Could the suite decide? | `INCONCLUSIVE` — exit `2` |
| Did the runner itself break? | `INTERNAL_ERROR` — exit `5` |

Only one of those six is a statement about the contract. `docs/cli.md` is the
reference; `estamora-core`'s `Blame` type is the enforcement, and every error
carries the class it was raised with, so an unreachable network can never be
reported as non-conformance.

## Extension points

Three things are designed to be added without touching the pipeline:

* **A dimension** — implement the evaluator in `estamora-assertions/src/dimensions/`
  and register it in the run loop. It receives the observation and returns
  `AssertionOutcome`s; it cannot reach the environment.
* **A report format** — implement it in `estamora-report` against the report model.
  The model is the schema's shape, so a new rendering cannot invent a field.
* **A profile** — no code at all. A profile is a document bundle in the
  specification repository; `docs/profile-format.md` describes the format and
  `CONTRIBUTING.md` describes how it is proposed.

`estamora-soroban` is the one layer that is not extensible from outside, and
deliberately: it is the boundary between untrusted input and a real execution
environment, and it is the only place that is allowed to have one.
