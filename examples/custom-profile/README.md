# Authoring a profile

The situation: the interface you need checked is not SEP-41, or is SEP-41 plus a
requirement your organisation adds. You need to write the profile.

This example is a complete, working bundle. It is deliberately small — four
read-only methods and two vectors — but every part of the format is used, and it
validates and executes:

```console
$ estamora validate --profile examples/custom-profile
$ estamora profile  --profile examples/custom-profile
$ estamora run      --profile examples/custom-profile --contract fixture:none
```

The last command produces a verdict against a contract that was already in the
repository, so there is nothing to build before seeing a profile work:

```text
Estamora 0.1.0 · token-reads@1.0 · local
contract  fixture:none
artifact  not available
profile   sha256:a56ad4139259705468917b22aef61efb93b1e15615ed73fedeed61b85f410a54 · vectors 2
────────────────────────────────────────────────────────────────────────
✓ balance-is-reported-exactly (passed)
✓ decimals-reports-the-scale (passed)
────────────────────────────────────────────────────────────────────────
checks by dimension
  interface      13 passed, 0 failed
  authorization   5 passed, 0 failed
  event           6 passed, 0 failed
  behavior        4 passed, 0 failed
  state           2 passed, 0 failed
  invariant       4 passed, 0 failed
  failure        not checked
  total          34 reported, 0 failed
────────────────────────────────────────────────────────────────────────
status CONFORMANT (exit 0)
```

`estamora validate` prints a warning, and the warning is correct: this bundle is at
`examples/custom-profile` rather than at `profiles/<id>/<version>/`, so a consumer
resolving `token-reads@1.0` *by name* would not find it. Naming the directory is a
supported way to measure a bundle under development, which is what this is. A bundle
being contributed is placed under `profiles/` before it is proposed.

Note also `failure  not checked`. This profile declares no failure mode, so the
dimension has no assertions — and it says so rather than printing `0 passed`, which
would read as a clean result. A suite that could not look has not found nothing.

## The shape of a bundle

A profile is a directory, not a file, and it is a directory because its parts have
different audiences:

| Document | What it states |
| --- | --- |
| `profile.yaml` | Identity, status, provenance, and the manifest of what follows. |
| `methods.yaml` | The interface: each method's arguments, types, semantics and mutability. |
| `authorization.yaml` | Who must authorize what, and what happens when they do not. |
| `events.yaml` | Which events must, may or must not be emitted, what they must contain, how many, in what order. |
| `behavior.yaml` | Preconditions and postconditions: what must hold before and after a call. |
| `invariants.yaml` | Properties that must survive an operation, whatever else it does. |
| `failures.yaml` | The ways an operation is allowed to fail, and what it must leave behind. |
| `vectors/` | The corpus: concrete situations and the outcome each one expects. |

Splitting them is not decoration. A behaviour rule and a failure rule about the
same method are read by different people — the first by whoever is checking the
contract's accounting, the second by whoever is checking its error paths — and a
reviewer who is asked to read one 1,500-line document reviews none of it. It also
makes the diff of a requirement change name the requirement's kind, which is the
difference between a review that catches a semantic change and one that does not.

## Start from `profile.yaml`

```yaml
estamora_spec_version: "1.0"

profile:
  id: token-reads
  version: "1.0"
  title: Token Read Surface
  status: experimental
  ...
```

`estamora_spec_version` is the format version, and it is the first thing the
loader reads. A bundle that declares a version this runner does not implement is
refused by name, because reading it with the wrong rules would be worse than not
reading it.

`status` is `experimental` here rather than `stable` or `standard`. A profile that
claims to be a standard asserts that its requirements are normative for somebody
else; `docs/versioning.md` in the specification repository is the definition, and it
is worth reading before choosing a status you cannot defend.

`provenance` is required and is the field contributors most want to leave empty. It
records where the requirements came from and, when a specification is ambiguous,
what this bundle decided the ambiguity means. A profile is a document with an
author, and a requirement whose origin is unstated cannot be reviewed.

## Requirements point at each other, and the loader checks that they do

A method lists the authorization rules, events, failures and behaviours that
concern it. The loader resolves every one of those references and fails on a name
that does not exist:

```yaml
methods:
  - id: balance
    authorization:
      - no-authorization-for-reads
    behaviors:
      - a-read-does-not-write
```

This is not a stylistic choice. A typo in that list would otherwise produce a
requirement that is declared, appears in `estamora profile` output, and is never
evaluated against anything — a check that silently does not happen. The runner
refuses to execute a bundle with a broken reference rather than executing it
partially, and `estamora validate` reports all of them at once rather than the
first.

## Stating an absence

The authorization rule in this bundle is a good example of a requirement that is
easy to leave out and expensive to omit:

```yaml
  - id: no-authorization-for-reads
    methods: [balance, decimals, name, symbol]
    actor:
      kind: none
    coverage:
      mode: at_most
      arguments: []
    unauthorized:
      kind: succeed
    wrong_actor:
      kind: succeed
```

Read it as a claim rather than as missing configuration: *no principal may be asked
to sign in order to read a balance*. A contract that demanded a signature to read a
public balance fails this rule, and in a bundle where the rule were simply absent
that contract would pass. `at_most` with an empty argument set is how "must not
demand authorization over anything" is written; `exact` is reserved for a rule that
names what its actor must cover, and the schema rejects an empty `exact` set.

The same principle applies to events, and it is why this bundle's `events.yaml` is
*not* empty. It declares `transfer`, `approve` and `burn` with `requirement:
forbidden`, and the vectors name them in `expected.events.forbidden`. Declaring them is three
claims, not one: that these events are concepts the profile recognises, that a
conformant implementation emits none of them on a read, and that a vector may
therefore forbid one by name and have the prohibition checked. An empty `events:`
makes none of them — a vector that forbade `transfer` would then be a corpus defect,
a reference to an event the profile never described — and the loader refuses it as
such rather than ignoring the vector.

## Vectors are where a profile becomes measurable

```console
$ estamora validate --profile examples/custom-profile
```

reports the corpus it resolved. Each vector states a starting ledger, the inputs,
and the outcome it expects; the profile says what that outcome must satisfy. The
same requirement is then applied to every vector that constructs it, which is why
adding a vector is a real increase in coverage and adding a requirement is a real
increase in rigour.

Write negative vectors. A corpus of successes establishes that a contract can do
the right thing, not that it refuses to do the wrong one, and the whole reason
Estamora distinguishes `EXPECTED_FAILURE` from `UNEXPECTED_SUCCESS` is that the
second is a conformance failure while the first is the point.

## Before you propose it

```console
$ estamora validate --profile <your-bundle>
```

must pass, and then the same bundle has to pass the specification repository's own
validation, because that repository is where profiles are normative and this runner
is not a second authority over them. `CONTRIBUTING.md` describes the proposal
process.
