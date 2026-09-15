# Security

## The sentence this document exists to support

**Passing Estamora conformance tests does not prove that a contract is secure.**

Estamora is a behavioural conformance tool. It answers one question: does this
contract behave as a named profile requires, over a named corpus? Everything in this
document follows from how narrow that is.

Conformance does not replace, and does not approximate:

* formal verification;
* a security audit;
* penetration testing;
* economic or game-theoretic analysis;
* vulnerability research;
* a review of who may mint, who holds the admin key, or whether the issuer's
  accounting is honest.

A contract can be fully conformant to SEP-41 and be trivially exploitable. SEP-41
describes what the token *interface* must do; it says nothing about how supply is
minted, who may mint it, whether the admin key is on a hardware device, or whether
the issuer will honour redemptions. Every one of those is outside what the profile
measures, and a report that did not make that visible would overstate itself.

This is why every terminal rendering ends with the limitation sentence, and why
`estamora run --help` states it too. A pass rate is the most quotable thing a
conformance run produces and the most easily read as a guarantee, so the sentence
that prevents that reading travels with the number rather than living only in a
document nobody opens.

Reading a conformance result correctly:

| The report says | It means | It does not mean |
| --- | --- | --- |
| `CONFORMANT` | Every required vector held. | The contract is correct, or safe, or audited. |
| `NON_CONFORMANT` | A requirement was contradicted. | The contract is exploitable, or that anything else is wrong. |
| `PARTIALLY_CONFORMANT` | Some vectors held and some did not. | The contract is *mostly* safe. |
| `INCONCLUSIVE` | The suite could not decide. | The contract is suspicious, or that it is fine. A skipped vector was not exercised. |
| `EXECUTION_ERROR` | The environment failed. | Anything at all about the contract. |
| `PROFILE_ERROR` | The requirements were unusable. | Anything at all about the contract. |

## Untrusted input

A conformance run has two kinds of untrusted input — documents and a contract — and
they are handled as such.

### Profiles and vectors are data, never code

A profile cannot cause arbitrary code to run. It is parsed as YAML, validated
against a published JSON Schema, and then interpreted by evaluators that only know
the expression algebra in `estamora-core`. There is no way to write a behavioural
rule that executes something: expressions name methods, inputs, state resources and
literals, and a name that does not resolve is an error.

Three specific hardening decisions, each of which exists because the alternative was
unsafe:

* **Fixture actors are the only addresses a vector may name.** A vector cannot write
  a literal address into an expression, so a document cannot make the address parser
  perform work on its behalf.
* **Argument values are parsed according to the type the profile declares**, not
  guessed from the document. A vector cannot widen an argument's type by writing a
  value of another shape, and a malformed value is a corpus error rather than
  something that reaches the execution host.
* **A file path in a bundle may not escape the bundle.** The manifest's document
  names are checked, and a profile that names a file outside its own directory is
  refused rather than read.

### The contract is the untrusted artifact

A contract being measured is code, and it can misbehave in ways that are not
conformance findings. Two things bound it:

* **The measurement host is a sandboxed Soroban execution environment.** A contract
  cannot reach the filesystem, the network or the process. This is the SDK's host,
  not something this project wrote, and it is the same host that runs the SDK's own
  test suite.
* **The artifact's size is bounded before it is read**, so a very large file is a
  contract resolution error rather than an allocation.

What is *not* bounded, and is worth knowing: a contract can loop, and a run against
a pathological contract can take as long as the host's budget allows. A conformance
run is not a denial-of-service defence, and should not be run against an artifact
from an untrusted source without one.

### Reports are output, not input, except when they are

`estamora report` and `estamora certify verify` read a JSON report that arrived from
somewhere. Those commands parse it strictly, refuse a document that is not a report,
and never render a document that was only partly understood — because a rendering of
something partly understood looks exactly like a rendering of a result. Text from a
report is written into Markdown and JUnit output, so a report is treated as
untrusted content rather than as a trusted template.

Nothing in the runner writes an ANSI escape into a report. Colour is a terminal
concern and is applied only where a terminal is the destination.

## The authority of a profile

A profile has authority over the requirements it defines, and nothing more.

* **A profile is not a standard because it says it is.** The `status` field is a
  claim the author makes, and `stable` is the claim that a behavioural change
  requires a version bump. Whether to trust it is a question about the author.
* **Provenance matters.** A profile records where its requirements came from and
  which upstream ambiguities it resolved. A profile whose requirements have no stated
  origin cannot be reviewed, and an unreviewable requirement is not a requirement.
* **Version pinning matters.** The profile version identifies the requirement set;
  the profile digest identifies the exact documents. A result that names neither is
  not interpretable, which is why both are in the report and in every receipt.
* **A malicious profile is not a standard.** Nothing stops someone from writing a
  profile that requires almost nothing, running it, and publishing a `CONFORMANT`
  receipt. That is why a receipt names the profile and its digest: the claim is
  always *conformant under this document*, and the document can be read.

The runner is not a second authority over profiles. It does not add requirements, it
does not relax them, and if it disagrees with a profile's own repository the profile
is right and the runner has the defect.

## Determinism

A conformance result is only meaningful if it is reproducible, so the run pins
everything ambient:

* the ledger sequence and close time, declared per vector;
* every account's authorization, declared per vector;
* dependency versions, pinned in `Cargo.lock` and committed;
* the Rust toolchain, pinned in `rust-toolchain.toml`;
* the instant a receipt records, an explicit input rather than a clock read at
  presentation time.

The consequence is that a stored report re-renders to a byte-identical document, and
a test asserts it. A tool whose verdict changed between runs without anything
changing would be reporting its own noise.

What is *not* pinned, and should be stated: the profile revision a run reads from a
specification checkout. Two runs against two different revisions of the same profile
version can disagree, and the digest in the report is what tells them apart.

## Reporting a vulnerability

See [`SECURITY.md`](../SECURITY.md) at the repository root for how to report a
security issue in Estamora itself. A finding in a *contract* that Estamora measured
is not an Estamora issue and should go to that contract's maintainers — Estamora
makes no claim to have found it, and a conformance pass does not mean there is
nothing to find.
