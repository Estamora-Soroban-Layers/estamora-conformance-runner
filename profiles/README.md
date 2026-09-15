# This directory is empty on purpose

Estamora is two repositories, and this is the one that *executes* profiles. The profiles
themselves — the normative statements of what conformance means — live in
[`estamora-conformance-spec`](https://github.com/Estamora-Soroban-Layers/estamora-conformance-spec),
under its `profiles/` directory:

```
estamora-conformance-spec/
├── profiles/
│   ├── sep-41/1.0/          the Soroban token interface
│   └── examples/            worked examples
├── vectors/                 shared vector families
└── schema/                  the normative JSON Schemas
```

## Why there is no copy here

A vendored profile would be a second authority over what conformance means, and the two
copies would drift. The drift would be invisible in the worst way: a contract measured
against the copy in this repository, and a report naming a profile version that the
specification's own copy defines differently, produce a verdict that cannot be
interpreted — and nothing in the output would say so.

So this repository reads a checkout of the specification, and every run names both the
profile version and the digest of the documents it actually read. That digest is what
distinguishes two revisions of the same version, which is the case that makes a vendored
copy dangerous rather than merely redundant.

## Where the profiles a run uses come from

`--profile` accepts three spellings, resolved in this order:

| Spelling | Resolved against |
| --- | --- |
| `id@version` | `<spec>/profiles/<id>/<version>/` |
| A path under `profiles/` | The specification checkout. |
| A directory holding a `profile.yaml` | Nowhere — it is read where it is. |

The specification checkout comes from `--spec`, then `$ESTAMORA_SPEC_REPO`, then the
sibling directory `../estamora-conformance-spec`. Nothing else is consulted, and the
runner never fetches anything, because which revision of which profile a verdict was
produced against has to be answerable from the report alone.

The third spelling is how a bundle under development is measured before it is
contributed — `examples/custom-profile/` in this repository is one, and
`docs/profile-format.md` describes the format.

## The one bundle that does live here

`fixtures/profiles/conformance-token/1.0/` is a profile bundle, and it is deliberately not
in this directory and not named as though it were normative. It is part of the *test
fixture set*: it measures the contracts in `fixtures/contracts/` so that the whole
pipeline can be exercised on a checkout with no sibling specification repository, and it
says so in its own `compatibility.notes`.

It is not a standard, it is not offered as one, and nothing about it is normative. A
profile that a consumer should measure against belongs in the specification repository,
where it can be reviewed as a requirement.
