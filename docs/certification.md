# Certification receipts

A receipt is a small document that commits to a conformance result, so that a claim
about a contract can be checked later without re-running anything and without
trusting the channel it arrived over.

It is not a certificate of correctness. `docs/security.md` is the longer version of
that sentence; the short one is at the end of every receipt rendering.

## What a receipt contains

```json
{
  "algorithm": "ed25519",
  "public_key": "A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=",
  "signature": "…",
  "receipt": {
    "estamora_spec_version": "1.0",
    "runner": { "name": "estamora", "version": "0.1.0" },
    "issued_at": "2026-09-15T00:00:00Z",
    "profile": {
      "id": "conformance-token",
      "version": "1.0",
      "digest": "sha256:02d961cd…"
    },
    "vectors": { "digest": "sha256:2bd5ad46…", "count": 4 },
    "target": {
      "contract": "fixture:none",
      "network": "local",
      "wasm_hash": null
    },
    "result": {
      "status": "CONFORMANT",
      "exit_code": 0,
      "summary": { "interface": { "passed": 10, "failed": 0, "total": 10 }, "…": {} }
    },
    "report_digest": "sha256:27ce864a…"
  }
}
```

Every field is there because a receipt without it cannot be interpreted:

| Field | Why |
| --- | --- |
| `profile.id`, `profile.version` | A verdict is a verdict *under a named profile*. The same contract can be conformant under one and non-conformant under another, and both are correct. |
| `profile.digest` | The version identifies the profile; the digest identifies the exact documents that were read. A requirement can change within a version if nobody bumps it, and the digest is what notices. |
| `vectors.digest`, `vectors.count` | The corpus is part of what was measured. A digest over a smaller corpus is a different claim. |
| `target.contract`, `target.network` | A result about an unnamed contract is evidence of nothing. |
| `target.wasm_hash` | Where an artifact was deployed its hash pins the exact code. `null` means no artifact was involved — a fixture — which is stated rather than left to inference. |
| `result.status`, `result.exit_code` | The verdict and the code a pipeline saw, so the two cannot disagree after the fact. |
| `result.summary` | The per-dimension tally, so a reader sees the shape of the result and not only its name. |
| `report_digest` | The commitment. Verification recomputes this over the report it is handed. |
| `runner.name`, `runner.version` | Which tool reached the verdict. A rule's meaning can change with the runner. |
| `issued_at` | When the commitment was made. Not part of the digest of the report; part of the receipt. |

## Issuing one

```console
$ estamora run --profile sep-41@1.0 --contract ./my-token.wasm \
    --report report.json --receipt receipt.json

$ estamora certify issue --report report.json --out receipt.json \
    --signing-key <32-byte-hex-seed> --issued-at 2026-09-15T00:00:00Z
```

Without `--signing-key` the receipt is **issued unsigned**: it still commits to the
report — `report_digest` is present and verification still checks it — but it
identifies no issuer, and `public_key` and `signature` are empty strings rather than
omitted. Verification says so plainly instead of presenting internal consistency as
evidence of authorship.

`--issued-at` accepts an RFC 3339 instant and defaults to now. A test or a
reproducible build passes it explicitly, because a receipt whose timestamp changes
on every run is not reproducible.

## Verifying one

```console
$ estamora certify verify --receipt receipt.json --report report.json \
    --public-key "$PUBLIC_KEY"
report digest sha256:27ce864a…
attributed to key 56475aa75463474c
verdict CONFORMANT (exit 0) under conformance-token@1.0 for fixture:none on local

A receipt attests that a named runner reached a named verdict under a named profile.
It is not a statement that the contract is correct or safe, and a valid signature
over a wrong conclusion is still a wrong conclusion.
```

Verification answers two separate questions and does not merge them:

1. **Does the receipt commit to this report?** The report digest is recomputed and
   compared. This is checkable with no key at all, and it is what catches a report
   edited after the fact.
2. **Who asserts it?** The signature is checked against the public key the caller
   supplies. Supplying it is what turns *the report matches* into *and this key
   asserted it*.

Without `--public-key`, verification still performs the first check and reports that
nobody is identified:

```text
attributed to nobody: no trusted key was supplied, so the signature was not checked;
a signature checked against a key carried in the same document establishes only that
the document agrees with itself
```

That sentence is the whole design of the command. A receipt carries its own public
key so that a reader can see who claims to have issued it, but **a key carried
beside a signature proves nothing about authorship** — anyone can generate a keypair
and sign anything. Trust in an issuer comes from the caller's own key distribution,
which is why the key is an input and never inferred.

## What a receipt does not establish

* It does not say the contract is correct. It says a named runner reached a named
  verdict under a named profile over a named corpus.
* It does not prove the run happened. A signature over a fabricated report verifies
  perfectly. What a receipt makes cheap is *checking that the report you were shown
  is the report that was committed to*.
* It does not establish that the profile is a standard. A profile is a document with
  an author, and a signed verdict under an obscure profile is a signed verdict under
  an obscure profile.
* It does not replace re-running. A receipt can be checked offline, which is why it
  is useful for a claim in a README or a registry entry; a claim about a contract
  deployed yesterday should be re-measured.

## Where this belongs

The specification repository defines the normative data model and the semantics of a
conformance result. This repository implements it. The receipt format is an
implementation of that model, and a change to what a receipt must contain is a
change to the specification, not to this crate.

`estamora-certification` also exposes receipt **verification** as a library, so a
future registry or a downstream tool can check a receipt without shelling out to the
CLI. That library is the reason the signing and verification logic lives in its own
crate rather than in the CLI: a third party should be able to verify an Estamora
receipt by depending on one small crate, not on the runner.
