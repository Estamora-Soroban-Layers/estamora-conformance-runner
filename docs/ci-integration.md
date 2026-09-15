# CI integration

Estamora is built to be run from a pipeline: a CLI with a documented exit-code
contract, a machine-readable report on standard output, and renderings for the CI
systems people already use.

## The contract with CI

| Code | Meaning | What a gate should do |
| --- | --- | --- |
| `0` | A conformant verdict was reached. | Merge or publish. |
| `1` | The contract violated a requirement. | Fail; the contract is at fault. |
| `2` | The suite could not decide. | Fail, and re-run or fix what made a vector unmeasurable. |
| `3` | The profile or corpus was unusable. | Fail the build; the requirements are at fault. |
| `4` | The environment failed. | Retry, then fail. The contract is not implicated. |
| `5` | The runner itself failed. | Report a bug. |
| `64` | The command line was wrong. | Fix the invocation. |

A failure never produces `0`, `1` or `2` unless it names the contract. That is the
property that makes `4` a reliable *retry this* signal: an unreachable node can
never be reported as non-conformance, and a broken profile can never be reported as
a broken contract.

`2` is separate from `1` on purpose. `INCONCLUSIVE` says the suite could not decide —
a vector was skipped, or an error meant it contributed nothing. Treating that as a
contract failure would report a broken runner as a broken contract; treating it as
success would report a suite that measured nothing as a passing one. If a gate wants
to be strict, `2` fails the job; if it wants to be forgiving about a flaky
environment, it can distinguish the two by code.

## Output formats

```console
$ estamora run --profile sep-41@1.0 --contract ./my-token.wasm \
    --format junit --out conformance.xml \
    --report conformance.json
```

| Format | For |
| --- | --- |
| `json` | The normative document — the one the specification publishes a schema for. Anything another tool consumes should read this. |
| `markdown` | A reviewer reading a pull request. |
| `junit` | An existing CI gate that already understands JUnit XML. |
| `text` | A terminal. The default for `run`. |

`--out` and `--report` are separate because a pipeline usually wants both from one
run. Only `--report` writes the normative document.

Two properties of the renderings are worth relying on:

* **An undecidable vector reaches JUnit as `error`, never as `failure`.** A CI system
  that treats `error` as infrastructure and `failure` as a defect will then classify
  correctly without knowing anything about Estamora.
* **`--report` is stable and re-renderable.** A stored JSON report re-renders to a
  byte-identical document, so a job can publish a machine-readable result and a
  reviewer can render the same result as Markdown without re-running anything.

## The Action

The shortest integration, and the one this repository maintains as an interface rather than
as an example. `action.yml` builds the tool from the revision the `uses:` reference pins,
runs it, and exits with the same code the run exited with:

```yaml
name: Estamora conformance

on:
  pull_request:
  push:
    branches: [main]

jobs:
  conformance:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7

      # The profiles are normative and live in the other repository. Pin the
      # revision: which revision of which profile a verdict was produced against
      # is part of what the verdict means.
      - uses: actions/checkout@v7
        with:
          repository: Estamora-Soroban-Layers/estamora-conformance-spec
          ref: v0.1.1
          path: estamora-conformance-spec

      - name: Build the contract
        run: cargo build --target wasm32v1-none --release -p my-token

      - name: Measure it
        uses: Estamora-Soroban-Layers/estamora-conformance-runner@v1
        with:
          spec: estamora-conformance-spec
          profile: sep-41@1.0
          contract: ./target/wasm32v1-none/release/my_token.wasm
          format: junit
          out: conformance.xml
          report: conformance.json
```

Two things about that step are worth knowing before it runs in a busy repository.

**It compiles the tool from source.** The action deliberately does not download a binary:
a verdict is only meaningful if the tool that produced it is identified, and building from
the revision `uses:` pins is what makes the revision the identity rather than whatever a
release channel happened to point at. The cost is a few minutes on a cold runner, which the
`Swatinem/rust-cache` step below removes on the second run. If the cost matters more than
the pinning, install a released binary instead and pass `install: false`.

**Pin a release, not a major tag, when a verdict has to reproduce.**
`@v1` follows the current major version, so it moves; `@v0.1.1` does not. Use the major tag
while you are adopting this, and a release tag once a result you have published has to be
reproducible from the workflow that produced it.

The action sets `status`, `exit-code` and `report` as outputs, so a later step can branch on
the verdict itself rather than on a bare failure:

```yaml
      - if: steps.conformance.outputs.status == 'INCONCLUSIVE'
        run: echo "::warning::Estamora could not decide; this is not a contract defect"
```

### Caching, and a longer example

```yaml
      - name: Cache
        uses: Swatinem/rust-cache@v2

      - name: Measure it
        uses: Estamora-Soroban-Layers/estamora-conformance-runner@v0.1.1
        with:
          spec: estamora-conformance-spec
          profile: sep-41@1.0
          contract: ${{ vars.CONTRACT_ID }}
          network: testnet
          tags: authorization events
          report: conformance.json

      - name: Publish the report
        if: always()
        uses: actions/upload-artifact@v7
        with:
          name: estamora-report
          path: conformance.json
```

### Without the Action

Not every pipeline is on GitHub, and a job that already installs its own tooling should not
install a second one. The released binary and the CLI are the portable surface:

```yaml
      - name: Install Estamora
        run: |
          curl -fsSL https://raw.githubusercontent.com/Estamora-Soroban-Layers/estamora-conformance-runner/v0.1.1/scripts/install-binary.sh | sh

      - name: Measure it
        run: |
          estamora run \
            --spec "$GITHUB_WORKSPACE/estamora-conformance-spec" \
            --profile sep-41@1.0 \
            --contract ./target/wasm32v1-none/release/my_token.wasm \
            --format junit --out conformance.xml \
            --report conformance.json
```

The installer verifies the archive against the release's published `SHA256SUMS` and refuses
to install anything it cannot check — which matters more here than anywhere else, because
the tool it is installing is the one whose verdict you are about to trust.

Note what the job does *not* do: it does not make the conformance result a condition
for anything else in the workflow, and it does not pass `--no-fail`. The exit code is
the gate. `--no-fail` exists for a job whose purpose is to publish a report on every
push regardless of the verdict; using it on a gate would make the gate decorative.

Two things to be careful about when wiring this up:

* **Pin the profile revision.** `--profile sep-41@1.0` names a profile version, and
  the *repository revision* it is read from is separate. A profile is a document with
  an author; measuring against a moving branch means the meaning of a passing run
  changes without a commit in your repository.
* **Do not gate on a skipped vector.** If a job reports `2` because vectors were
  skipped, the fix is to give the run a way to establish the vector's world, not to
  relax the gate. `docs/local-testing.md` explains the case.

## Running it in this repository's own CI

```console
$ ./scripts/run-ci.sh
```

formats, lints, builds, tests, validates the in-repository profile and corpus,
runs the cross-repository tests when a specification checkout is present, and runs
the release checks. It is the same sequence the workflow runs, so a contributor can
reproduce a red job without pushing.
