# Measuring a local contract

The situation: you have a compiled Soroban contract and a claim about which
interface it implements. You want a verdict, on your machine, without deploying
anything anywhere.

## Point the runner at the specification

The runner executes profiles, and the profiles are normative documents that live
in `estamora-conformance-spec`. It finds that checkout in one of three ways, in
order:

1. `--spec <PATH>`, if you named one;
2. `ESTAMORA_SPEC_REPO`, if it is set;
3. the sibling directory `../estamora-conformance-spec`, because the two
   repositories are developed beside each other.

Nothing else is consulted, and the runner never fetches anything: which revision
of which profile a verdict was produced against has to be answerable from the
report alone.

```console
$ export ESTAMORA_SPEC_REPO=/path/to/estamora-conformance-spec
```

## Run it

```console
$ estamora run \
    --profile sep-41@1.0 \
    --contract ./target/wasm32-unknown-unknown/release/my_token.wasm \
    --report my-token.conformance.json
```

`--profile` accepts three spellings of the same thing: `id@version`, resolved
under the specification checkout's `profiles/`; a path under that directory; and
a directory containing a `profile.yaml`, which is how a bundle under development
is measured before it is contributed.

`--contract` accepts a `.wasm` path, a contract identifier together with
`--network`, or `fixture:<name>` for one of the runner's own built-in contracts.
The fixture selector exists so that this example runs before you have written any
contract at all:

```console
$ estamora run --spec fixtures \
    --profile conformance-token@1.0 \
    --contract fixture:none
```

`--spec fixtures` names this repository's own specification tree, which is laid
out the way a checkout of `estamora-conformance-spec` is: `profiles/<id>/<version>/`.
That is why the fixture profile resolves by name, exactly as `sep-41@1.0` resolves
against the real repository. The run above is reproduced in full below, so it can be
compared against what you get.

`fixture:none` is a contract with no injected defect. The other fixture names each
inject exactly one, and are listed for you when you ask for a name that does not
exist.

## Read the output

```text
────────────────────────────────────────────────────────────────────────
Estamora 0.1.0 · conformance-token@1.0 · local
contract  fixture:none
artifact  not available
profile   sha256:02d961cdf2d90f33055019542fa23854a9f483596de8cb1059197e8d2c3c6a9c · vectors 4
────────────────────────────────────────────────────────────────────────
✓ balance-is-reported (passed)
✓ transfer-beyond-the-balance-fails (passed)
    · rule-inapplicable-transfer-moves-the-exact-amount …
    · authorization-unobservable-sender-authorizes-transfer …
✓ transfer-moves-the-exact-amount (passed)
    · rule-inapplicable-transfer-beyond-the-balance-fails …
✓ transfer-without-authorization-fails (passed)
    · rule-inapplicable-transfer-moves-the-exact-amount …
    · rule-inapplicable-transfer-beyond-the-balance-fails …
    · authorization-unobservable-sender-authorizes-transfer …
────────────────────────────────────────────────────────────────────────
checks by dimension
  interface      10 passed, 0 failed
  authorization  10 passed, 0 failed
  event          10 passed, 0 failed
  behavior       7 passed, 0 failed
  state          7 passed, 0 failed
  invariant      5 passed, 0 failed
  failure        10 passed, 0 failed
  total          59 reported, 0 failed
────────────────────────────────────────────────────────────────────────
status CONFORMANT (exit 0)
────────────────────────────────────────────────────────────────────────
Conformance is not security. This result says the contract satisfied the named
profile over the named corpus; it does not say the contract is free of
vulnerabilities, and passing does not replace a formal verification, an audit or a
penetration test. A profile is a document with an author: which profile, at which
revision, and from where, are part of what this result means.
```

The `·` lines are elided above for width; each one is a complete sentence in the
real output. They are the run telling you about a check it could **not** make.
`authorization-unobservable-sender-authorizes-transfer` is the interesting one: the
vector expects the call to be refused, so the host unwinds the record of what was
authenticated and there is nothing left to compare the profile's authorization
requirement against. Reporting that as a pass would be claiming evidence that does
not exist, and reporting it as a failure would blame the contract for the
environment's behaviour; it is reported as a finding instead, and the reason the
call was refused is checked by the outcome assertion that the same vector carries.

Only checks that did not hold are listed by default. `--verbose` lists every check
that was made, one per line with the dimension it belongs to, which is what to reach
for when a run passed and you want to know what it therefore established. The tally
prints either way and counts assertions only, so a dimension with no assertions reads
`not checked` rather than `0 passed` — the two are different statements, and a suite
that could not look is not a suite that found nothing.

## What you get

`--report` writes the normative JSON document: the one `estamora-conformance-spec`
publishes a schema for, the one a receipt commits to, and the one another tool
should read. `--out` writes the human rendering and defaults to standard output.
They are separate flags on purpose: a pipeline usually wants both from one run,
and a terminal rendering that has been parsed by a script is a mistake waiting to
happen.

## Exit codes

| Code | Status | What a gate should do |
| --- | --- | --- |
| `0` | `CONFORMANT` | Publish or merge. |
| `1` | `NON_CONFORMANT`, `PARTIALLY_CONFORMANT` | Fail; the contract is at fault. |
| `2` | `INCONCLUSIVE` | Fail; re-run the suite. |
| `3` | `PROFILE_ERROR` | Fail the build; the profile is at fault. |
| `4` | `EXECUTION_ERROR` | Retry or fail; the environment is at fault. |
| `5` | — | An internal error, including a report that is not a report. |

The two statuses on `1` are different findings. `NON_CONFORMANT` means a
requirement was contradicted. `PARTIALLY_CONFORMANT` means some vectors passed
and some failed, which is the ordinary result of measuring a real contract
against a profile that spans more methods than it has finished. `INCONCLUSIVE`
is deliberately not on `1`: it says the suite could not decide, so a gate that
treated it as a contract failure would be reporting a broken runner as a broken
contract.
