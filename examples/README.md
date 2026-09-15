# Examples

Five worked examples, in the order a reader should meet them.

| Directory | What it demonstrates |
| --- | --- |
| [`local-contract/`](local-contract/) | The shortest path from a compiled contract to a verdict: measure a `.wasm` against a profile. |
| [`testnet-contract/`](testnet-contract/) | Measuring a deployed contract over a network, and why a network failure is not a contract failure. |
| [`sep-41/`](sep-41/) | The real standard, read from `estamora-conformance-spec`, against a contract that implements it. |
| [`failing-contract/`](failing-contract/) | What a non-conformant verdict looks like, produced by contracts that are wrong in one way each. |
| [`custom-profile/`](custom-profile/) | A complete profile bundle written from scratch, and the smallest one that is still real. |

Every command below is run from the root of this repository. Each example states
what it needs — a compiled contract, a network, a checkout of the specification —
and none of them needs all three.

## What the examples are not

They are not a tour of the CLI for its own sake, and they do not demonstrate
features that exist only to be demonstrated. Each one exists because it is a
situation a user is actually in: someone has a contract and a claim about it, and
wants to know whether the claim holds.

They are also not assertions that any particular contract is *correct*. An
example that ends in `CONFORMANT` says that a contract satisfied a named profile
over a named corpus, and nothing else. `docs/security.md` is the longer version
of that sentence.

## Running them without a contract

Two of the commands an example uses need no contract at all, and are worth knowing
before reading any of them:

```console
$ estamora profile --spec fixtures --profile conformance-token@1.0
```

describes what a profile requires, and

```console
$ estamora validate --spec fixtures --profile conformance-token@1.0
```

checks that a profile and its vector corpus are internally consistent. Both are
useful in a pull request against a profile, which is the case they were written
for.

The `--spec fixtures` is this repository measuring its own specification tree. An
example that measures a contributor's contract passes `--spec` a checkout of
`estamora-conformance-spec` instead, or sets `ESTAMORA_SPEC_REPO` and leaves the
flag off.
