# Security policy

## Conformance is not security

**Passing Estamora conformance tests does not prove that a contract is secure.**

Estamora answers one question: does this contract behave as a named profile requires,
over a named corpus? A contract can be fully conformant to SEP-41 and be trivially
exploitable, because SEP-41 describes the token *interface* and says nothing about
who may mint, whether the admin key is safe, or whether the issuer's accounting is
honest.

Estamora does not replace formal verification, a security audit, penetration testing,
economic analysis or vulnerability research, and it makes no claim to have found
anything. `docs/security.md` is the detailed treatment; this file is about reporting a
problem in Estamora itself.

## Reporting a vulnerability in Estamora

Report privately, not in a public issue. Use GitHub's private vulnerability reporting
on this repository (Security → Report a vulnerability), or contact a maintainer listed
in `CODEOWNERS`/the repository's maintainers directly.

Please include:

* what you did, and the exact command line;
* what you expected and what happened;
* the runner version (`estamora --version`) and the profile digest
  (`estamora validate --profile …`), because both are part of what a result means;
* a minimal reproduction where you can produce one — a bundle, a vector or an
  artifact, reduced as far as it goes.

We will acknowledge within a few days and keep you updated. Please give us a
reasonable window before disclosing publicly; we would rather ship a fix and credit
you than argue about a deadline.

## What we consider a vulnerability

Anything that makes Estamora's output untrue, or that lets an untrusted input reach
further than it should:

* a **fabricated pass** — a check reported as satisfied when it was not made, or a
  verdict that does not follow from the checks;
* an untrusted document that can cause arbitrary code to run, escape the bundle
  directory, or exhaust memory or time beyond the documented bounds;
* a **misattributed failure** — an environment failure reported as a contract
  failure, or the reverse, so that a CI gate acts on the wrong thing;
* a **receipt that verifies when it should not** — a signature accepted under the
  wrong key, a report digest that does not match, or a receipt presented as
  identifying an issuer when none was supplied.

Reported findings of this kind are the ones we treat as security issues, because each
one degrades the only property Estamora sells: that its output can be relied on for
what it says and not for more.

## What we do not consider a vulnerability here

* **A bug in a contract Estamora measured.** Estamora makes no claim to have found it
  and none to have prevented it. That report belongs to the contract's maintainers.
* **An obscure profile that requires almost nothing.** Anyone may write a profile and
  run it. A receipt names the profile and its digest precisely so that a reader can
  judge the claim; a `CONFORMANT` verdict under a permissive profile is an accurate
  verdict under a permissive profile.
* **The absence of a network transport in this build.** It is documented, it is
  refused with a named reason, and it exits as an environment failure rather than as
  anything about a contract. See `docs/testnet-testing.md`.
* **A refusal to execute a profile this runner does not understand.** Refusing by
  name is the designed behaviour.

## Supported versions

The latest released minor version receives security fixes. Pre-`1.0`, a fix may ship
in a patch release and may change behaviour where the behaviour was wrong; a change
to what a verdict means is documented in `CHANGELOG.md`.
