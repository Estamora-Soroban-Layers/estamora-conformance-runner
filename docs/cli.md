# CLI reference

```
estamora [--spec <PATH>] [--verbose] <COMMAND>
```

Six commands, each answering a different question:

| Command | Question |
| --- | --- |
| `run` | Does this contract behave as this profile requires? |
| `inspect` | What does this contract expose? |
| `profile` | What does this profile require? |
| `validate` | Are these requirements usable at all? |
| `report` | Render a stored result into something a person can read. |
| `certify` | Commit to a result, and check a commitment. |

The split exists because three of them are answerable without executing anything. A
run that conflated them would have no way to say *the profile is wrong* as distinct
from *the contract is wrong*, and that distinction is the whole value of the tool.

Every command returns an exit code rather than exiting, so the whole surface is
testable as a library and there is exactly one place a process status is set.

## Global options

| Option | Effect |
| --- | --- |
| `--spec <PATH>` | The specification checkout to read requirements from. |
| `--verbose` | List every check performed rather than only the ones that did not hold. |

`--spec` defaults to `$ESTAMORA_SPEC_REPO`, then to the sibling directory
`../estamora-conformance-spec`, because the two repositories are developed beside
each other. Nothing else is consulted, and the runner never fetches anything: which
revision of which profile a verdict was produced against has to be answerable from
the report alone.

Standard output carries the document; diagnostics go to standard error. A
machine-readable report on standard output is how a pipeline consumes a run, and a
log line printed beside it is how that pipeline breaks in a way nobody notices
until the day it matters.

## `estamora run`

```
estamora run --profile <PROFILE> --contract <CONTRACT> [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `--profile <PROFILE>` | `id@version`, a path under the checkout's `profiles/`, or a directory holding a `profile.yaml`. |
| `--contract <CONTRACT>` | `fixture:<name>`, a path ending in `.wasm`, or a contract identifier together with `--network`. |
| `--network <NETWORK>` | The network a deployed contract lives on. Required for a contract identifier. |
| `--tag <TAG>` | Select only the vectors carrying every one of these tags. Repeatable. |
| `--format <FORMAT>` | `text` (default), `json`, `markdown` or `junit`. |
| `--out <PATH>` | Where the rendering is written. Defaults to standard output. |
| `--report <PATH>` | Additionally write the normative JSON report here. |
| `--receipt <PATH>` | Write a certification receipt here. |
| `--signing-key <HEX>` | Sign the receipt with this 32-byte hex seed. |
| `--no-fail` | Exit `0` whatever the verdict, having still recorded it. |

`--out` and `--report` are separate flags on purpose. A human-readable rendering on
a terminal and the document another tool consumes are different artefacts, and a
pipeline usually wants both from one run.

`--no-fail` is for a job whose purpose is to *produce* a report — publishing one on
every push, say — rather than to gate a merge. It changes the exit code and nothing
else: the verdict is still recorded, still reported, and still visible.

A `--tag` that selects nothing is reported as such rather than passing quietly. A
filter that matched no vectors has not established anything, and a suite that
reports a clean run over an empty selection is worse than one that fails.

## `estamora inspect`

```
estamora inspect --contract <CONTRACT> [--network <NETWORK>] [--format <FORMAT>] [--out <PATH>]
```

Reads the contract's declared interface — every method, with its parameter and
return types — without executing anything against it.

An inspection that did not happen is never reported as an empty interface. The
difference between *exposes nothing* and *could not be read* is the difference
between a defect and an absence of information, and a caller that cannot tell them
apart will treat an unreachable contract as a conformant one.

The rendering ends by saying what an interface is worth, because the number to
resist reading as a guarantee is the tick rate.

## `estamora profile`

```
estamora profile --profile <PROFILE> [--format <FORMAT>] [--out <PATH>]
```

Describes a profile: its identity, status, upstream specification, digest, the
methods it requires with their signatures, and how many of each other kind of
requirement it declares.

A profile that is malformed is not described. A summary of a document that could
not be parsed would be a summary of something else.

## `estamora validate`

```
estamora validate --profile <PROFILE> [--format <FORMAT>] [--out <PATH>]
```

Validates a bundle against the published JSON Schemas and resolves every
cross-reference between its documents, without deploying anything. It prints the
profile digest, the corpus digest, the requirements declared, the vectors resolved,
and every finding.

This is the command to run in a pull request against a profile, and it is what CI
runs against the specification repository's profiles.

Validation accumulates findings rather than stopping at the first, so a contributor
sees every defect in one run, and it exits `3` if any of them was an error.

## `estamora report`

```
estamora report --report <PATH> [--format <FORMAT>] [--out <PATH>] [--no-fail]
```

Re-renders a stored JSON report. `--format` defaults to `markdown` here rather than
to `text`, because the reason to render a stored report is usually to put it
somewhere a reviewer will read it.

It exits with the code the *stored run* recorded, so a job that renders a report
into a pull request comment still fails when the run it is rendering failed.
`--no-fail` overrides that.

Read-and-re-render is exact: the document a run writes and the document produced by
rendering its stored form again are byte-identical, which `integration-tests/`
asserts against a stored report with a pinned timestamp.

## `estamora certify`

```
estamora certify issue  --report <PATH> --out <PATH> [--signing-key <HEX>] [--issued-at <RFC3339>]
estamora certify verify --receipt <PATH> --report <PATH> [--public-key <BASE64>]
```

`issue` produces a receipt committing to a report: the profile and its digest, the
corpus digest and vector count, the target and its Wasm hash where available, the
status, the exit code, the per-dimension summary, and a digest of the report
itself.

`verify` checks a receipt against the report it claims to be about, and reports
which key it is attributed to. Without `--public-key` it still checks the report
and says plainly that nobody is identified — a signature checked against a key
carried in the same document establishes only that the document agrees with itself.
`docs/certification.md` is the longer version.

## Exit codes

| Code | Meaning | What a gate should do |
| --- | --- | --- |
| `0` | The run reached a conformant verdict. | Publish or merge. |
| `1` | The contract violated a requirement (`NON_CONFORMANT` or `PARTIALLY_CONFORMANT`). | Fail; the contract is at fault. |
| `2` | The suite could not decide (`INCONCLUSIVE`). | Fail; re-run, or fix what made a vector unmeasurable. |
| `3` | The profile or corpus was unusable. | Fail the build; the requirements are at fault. |
| `4` | The environment failed. | Retry, then fail; the contract is not implicated. |
| `5` | The runner itself failed. | Report a bug. |
| `64` | The command line was wrong. | Fix the invocation. |

A failure never produces `0`, `1` or `2` unless it names the contract, which is
what stops an unreachable network from being reported as non-conformance. `64` is
taken from `sysexits.h` so that a wrapper script can recognise a usage mistake
without knowing this program.

`1` covers two different findings, and the report says which. `NON_CONFORMANT` is a
requirement contradicted outright; `PARTIALLY_CONFORMANT` is some vectors passing
and some failing, which is the ordinary result of measuring a real contract against
a profile that spans more of an interface than it has finished.

## Environment

| Variable | Effect |
| --- | --- |
| `ESTAMORA_SPEC_REPO` | The specification checkout, when `--spec` is not given. |
| `ESTAMORA_RPC_URL` | The endpoint to read a deployed contract from, overriding the one `--network` names. This is how a network this build does not know by name is reached; a blank value is treated as absent. |
| `NO_COLOR` | Disables colour in terminal renderings. |
| `RUST_LOG` | `tracing` filter for diagnostics written to standard error. |

Colour is never written into a report or a receipt, whatever the terminal is.
