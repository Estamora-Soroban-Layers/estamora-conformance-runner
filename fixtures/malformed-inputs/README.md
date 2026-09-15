# Malformed inputs

Documents a conformance run must **refuse**, each wrong in exactly one way, and the
classification each refusal must carry.

They are not stored as whole bundles. A bundle is seven documents and a corpus, and
copying it once per defect would give eight near-identical trees in which the single
difference that matters — the line that is wrong — is the hardest thing to find.

Instead each directory here holds only the documents that differ, laid out at the paths
they occupy **inside** a bundle. A test copies the valid bundle from
`../profiles/conformance-token/1.0`, copies one of these directories over it, and asserts
what the run does with the result — so each fixture directory is a diff against a known
good bundle, and the defect is the only thing in it.

| Fixture | Wrong in | Must be refused as |
| --- | --- | --- |
| `profile-wrong-spec-version/` | declares a specification format this runner cannot execute | `PROFILE_ERROR`, exit `3` |
| `profile-missing-document/` | lists a document that is not in the directory | `PROFILE_ERROR`, exit `3` |
| `profile-broken-reference/` | names an event no document declares | `PROFILE_ERROR`, exit `3` |
| `vector-missing-field/` | a vector with no expected outcome at all | `VECTOR_ERROR`, exit `3` |
| `report-not-a-report.json` | valid JSON that is not a conformance report | `REPORT_ERROR`, exit `5` |

The last one is a file rather than a directory because it is not part of a bundle: it is
handed to `estamora report` and `estamora certify verify`, which read a stored document
rather than a profile.

**Why that one is exit `5` and not `3`.** The exit-code contract divides failures by who
they blame. A profile or a corpus is the specification's, so a document problem there is
`3`. An unreachable network is the environment's, so it is `4`. A document that is not a
conformance report is neither: the specification is fine and the environment was never
used, so the taxonomy has no class that names it and the catch-all applies. It is pinned
by a test rather than left to be discovered, and a script that keyed on exit code alone
would learn about it the first time it was handed the wrong file.

None of these may produce an exit code of `1`. That is the point of the fixture set: a
caller reading the exit code has to be able to tell "the contract violated a requirement"
from "the inputs could not be read", and a runner that reported the second as the first
would blame a contract for a document.
