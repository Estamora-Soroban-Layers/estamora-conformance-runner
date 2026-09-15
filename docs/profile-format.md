# The profile format

A profile is the document that defines conformance. This file describes the format
the runner consumes; the normative definitions of the fields are the JSON Schemas in
[`estamora-conformance-spec/schema/`](https://github.com/Estamora-Soroban-Layers/estamora-conformance-spec/tree/main/schema),
and the authoring guide is that repository's `docs/profile-authoring.md`. Where
this file and a schema disagree, the schema is right.

## A bundle, not a file

A profile is a directory:

```
profiles/<id>/<version>/
├── profile.yaml        identity, status, provenance, and the manifest
├── methods.yaml        the interface
├── authorization.yaml  who must authorize what
├── events.yaml         which events must, may or must not be emitted
├── behavior.yaml       preconditions and postconditions
├── invariants.yaml     properties that must survive an operation
├── failures.yaml       the ways an operation is allowed to fail
└── vectors/            the corpus, one directory per operation
```

The reason it is a directory is that its parts have different audiences. A
behaviour rule and a failure rule about the same method are read by different
people, and a reviewer asked to read one 1,500-line document reviews none of it. It
also makes the diff of a requirement change name the requirement's kind — the
difference between a review that catches a semantic change and one that does not.

**Every document is required, including the empty ones.** A bundle must contain a
`failures.yaml`, and a profile that declares no failure mode writes `failures: []`.
The alternative makes *this profile states that nothing may fail* and *somebody
forgot to declare the failures* the same document to a reader and to the loader, and
the second is the failure mode worth refusing: a requirement that went missing by
accident is indistinguishable, in a report, from one that was never made.

## Identity and the manifest

```yaml
estamora_spec_version: "1.0"

profile:
  id: sep-41
  version: "1.0"
  title: Soroban Token Interface
  status: stable
  ...

includes:
  methods: methods.yaml
  authorization: authorization.yaml
  events: events.yaml
  behavior: behavior.yaml
  invariants: invariants.yaml
  failures: failures.yaml
  vectors:
    - transfer
    - approve
  shared_vectors:
    - common
```

`estamora_spec_version` is the version of the *format*, and it is the first thing
the loader reads. It is independent of the profile version and of any runner
version, and it is bumped only when the document grammar changes in a way a consumer
must know about. A bundle declaring a format this runner does not implement is
refused by name, because reading it with the wrong rules would be worse than not
reading it.

`status` is one of four lifecycle stages, and it is a claim the profile has to be
able to defend:

| Status | Means |
| --- | --- |
| `draft` | May change without notice. Not something to measure a deployed contract against. |
| `experimental` | Executable and unreviewed. The case for a profile under development. |
| `stable` | A behavioural change requires a version bump. |
| `deprecated` | Must name a successor in `superseded_by`. |

`compatibility.interface` is `full` or `partial`, and `compatibility.notes` is
required to be **non-empty**. `provenance` records the class of authority the
requirements came from, the exact revision they were read from, and — required to
be non-empty — the ambiguities the profile resolved and how. Estamora forbids
inventing requirements, so a departure from the literal upstream text has to be
recorded rather than absorbed. A bundle whose requirements have no stated origin
cannot be reviewed.

`shared_vectors` names vector sets under the *repository-level* `vectors/`
directory, which is how a requirement stated once — a transfer beyond the balance
fails, an unknown account's balance is zero — is consumed by every profile that
needs it. A profile that had to restate them would drift from the others, and the
drift would be invisible.

## Methods

```yaml
methods:
  - id: transfer
    name: transfer
    requirement: required
    summary: Move an amount from one account to another.
    description: >-
      ...
    args:
      - name: from
        type: { kind: prim, name: address }
        semantics: The account the tokens are drawn from.
        authorization:
          required: true
          semantics: The holder must sign for the amount.
    returns:
      type: { kind: prim, name: void }
      semantics: Nothing; the effect is the balance change.
    mutability: mutates
    invocation: read_write
    authorization: [transfer-requires-holder-authorization]
    events: [transfer]
    failures: [insufficient-balance, unauthorized-caller]
    behaviors: [transfer-moves-exact-amount]
```

A method declares its own signature, and then **references** the rules that concern
it. Every one of those references is resolved by the loader, which fails on a name
that does not exist.

That is not a stylistic choice. A typo in one of those lists would otherwise produce
a requirement that is declared, appears in `estamora profile` output, and is never
evaluated against anything — a check that silently does not happen. The runner
refuses to execute a bundle with a broken reference rather than executing it
partially, and `estamora validate` reports all of them at once rather than the first.

`requirement` is `required`, `optional` or `forbidden`. A method marked `optional`
is measured and reported; a vector that fails against it is recorded and, by itself,
never decides a verdict. This is how a profile says *a conforming implementation may
not have this*.

## Authorization

Each rule states an actor, the coverage the actor's signature must have, and what
happens without it:

```yaml
authorization_rules:
  - id: transfer-requires-holder-authorization
    methods: [transfer, transfer_from]
    actor:
      kind: argument
      name: from
    coverage:
      mode: exact
      arguments: [from, amount]
    unauthorized:
      kind: fail
      failure: unauthorized-caller
    wrong_actor:
      kind: fail
      failure: unauthorized-caller
    replay_sensitive: false
```

There are three actors: `argument` (a principal named by an argument value),
`none` (no principal may be asked to sign, which is what a read-only surface
declares), and the caller. `coverage.mode` is `exact` or `at_most`. `exact` names
what the signature must cover and is rejected by the schema when the set is empty;
`at_most` with an empty set is the encoding of "must not demand authorization over
any argument".

`unauthorized` and `wrong_actor` are separate fields because they are separate
requirements. A contract that accepts an unsigned call and a contract that accepts a
signature from the wrong account are wrong in different ways, and a profile that
could not tell them apart would report one of them as conformant.

## Events

```yaml
events:
  - id: transfer
    name: transfer
    requirement: required
    occurrence: on_success
    topics:
      - index: 0
        type: { kind: prim, name: symbol }
        binding: { kind: literal, value: transfer }
        semantics: The event discriminator.
    data:
      format: either
      fields:
        - name: amount
          type: { kind: prim, name: i128 }
          binding: { kind: input, name: amount }
          semantics: The quantity moved.
          optional: false
    cardinality: { min: 1, max: 1 }
    ordering: []
    correlations: [balances-conserved-by-transfer]
```

`requirement` here is what makes the event document worth writing even for a
read-only surface: `forbidden` declares an event the profile recognises and requires
to be absent, which is what lets a vector name it in `expected.events.forbidden` and
have the prohibition checked. An empty `events:` would make that vector a corpus
defect — a reference to an event the profile never described.

`binding` is the mechanism that keeps a requirement from being a hard-coded
literal. An event field can be bound to an operation input, to a fixture actor, to a
state value read after the call, to another read, or left `unconstrained` when only
its presence and shape matter. A profile that bound the amount to `250` would pass a
contract that emitted a stale constant.

`format: either` exists because SEP-41 explicitly permits both a scalar/vec form and
a map form for several events. Forcing one reading would fail conforming contracts,
which is a worse outcome than accepting both.

Matching is on the declared `name` and nothing else, so a requirement is not a test
of one implementation language.

## Behaviour, state, invariants and failures

Behaviour states postconditions as expressions over the before and after worlds:

```yaml
behaviors:
  - id: transfer-moves-exact-amount
    method: transfer
    kind: success
    preconditions:
      - kind: greater_or_equal
        left: { kind: input, name: amount }
        right: { kind: literal, value: "0" }
    postconditions:
      - kind: decrease
        left: { kind: state_before, resource: balance, target: from }
        right: { kind: input, name: amount }
```

Because the left side names the world *before* the call and the amount names the
operation's own input, the rule holds for every starting balance rather than for the
amounts a vector happens to use. A rule is also scoped by the outcome the vector
expects: a rule that describes the success path is reported as inapplicable — with
the reason — on a vector that expects a refusal, rather than being applied and
failing a conforming contract.

Invariants are declared once with a scope (the methods and outcomes they range over)
and referenced by id. Outside their scope they are reported as inapplicable, because
an invariant evaluated over an empty set has been skipped, not verified.

Failures name a **semantic category** rather than an implementation's error string,
so a profile is not brittle against an implementation that numbers its errors
differently. Where a standard fixes a category name, the profile uses it; where it
does not, the profile states the category and documents the interpretation in
`provenance.interpretation_notes`.

## Vectors

A vector is a concrete situation and the outcome it expects:

```yaml
id: transfer-basic-001
profile: sep-41
profile_version: "1.0"
kind: positive
method: transfer
tags: [transfer, positive]

fixtures:
  actors:
    - { name: alice, kind: account }
  balances: { alice: "1000", bob: "500" }
  total_supply: "1500"
  ledger:
    sequence: 1000
    timestamp: "2026-01-15T12:00:00Z"

inputs:
  from: { kind: actor, ref: alice }
  to: { kind: actor, ref: bob }
  amount: "250"

authorization:
  actors: [alice]
  expected: accepted

expected:
  outcome: success
  state_assertions:
    - resource: { kind: balance, account: alice }
      predicate:
        kind: equal
        left: { kind: read, method: balance, args: [{ kind: actor, ref: alice }] }
        right: { kind: literal, value: "750" }
  events:
    required:
      - event: transfer
```

`inputs` are parsed **according to the type the profile declares**, not guessed from
the document: a vector writes an amount as `"250"` and the profile says the argument
is an `i128`, so the runner parses it as one. A vector cannot widen an argument's
type by writing a value of another shape.

Fixture actors are the only addresses a vector may name. There is no way to write a
literal address into an expression, which is what stops a document from making the
address parser perform work on its behalf.

Negative vectors are mandatory in practice. A corpus of successes establishes that a
contract can do the right thing, not that it refuses to do the wrong one, and the
whole reason Estamora separates `expected failure` from `unexpected success` is that
the second is a conformance failure while the first is the point.

## Checking a profile before you propose it

```console
$ estamora validate --profile <path-or-id@version>
```

must pass, and then the same bundle has to pass the specification repository's own
validation, because that repository is where profiles are normative and this runner
is not a second authority over them. `CONTRIBUTING.md` describes the proposal
process.

## What the runner refuses to do with a bundle

| Situation | Result |
| --- | --- |
| A format version this runner does not implement | `PROFILE_ERROR`, naming the version. |
| A manifest naming a document that is absent | `PROFILE_ERROR`, naming the file. |
| A requirement referring to something undeclared | An error, not a warning. |
| A vector with no expected outcome, or referring to an undeclared event | `VECTOR_ERROR`. |
| A malformed document, or one that does not parse | `PROFILE_ERROR`; nothing is described from a document that was partly understood. |

Never a verdict. A runner that tolerates an unreadable profile evaluates a contract
against requirements it never read, and then reports a result — which is the exact
failure mode Estamora exists to prevent.
