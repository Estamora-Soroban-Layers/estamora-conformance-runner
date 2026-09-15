<!--
Thanks for the change. Two things make a pull request here much easier to review, and
both are about the same idea: this tool's output is a verdict, so a change to what it
reports is a change to what its users can conclude.

  1. Say *why* — and, where you can, what would have to be true for the change to be
     wrong. That is what a reviewer can actually test.
  2. Say what a run reports differently after it. If nothing does, say so; that is a
     perfectly good answer and it tells a reviewer not to look for it.
-->

## What this changes

<!-- One or two sentences. What is different for someone using Estamora? -->

## Why

<!--
The reasoning. If this fixes a defect, what was the defect, and how did you find it? If
it adds a capability, what could a user not find out before?
-->

## Which repository this belongs to

- [ ] The runner (how a requirement is executed and measured)
- [ ] The specification (what conformance means) — a proposal for
      `estamora-conformance-spec`, linked here rather than made here

<!--
If a change would make the runner disagree with the specification, it is wrong, and the
fix is a pull request there. This repository contains no normative claims.
-->

## Effect on a verdict

- [ ] No change to what any run reports
- [ ] Changes what a run reports, and the new output is correct because …

<!--
A change that can change a verdict is treated as a breaking change even when no API
changes, and it needs a test that would have failed before it. Say here which test.
-->

## Checks

- [ ] `./scripts/run-ci.sh` passes
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes

## If this touches a conformance dimension

- [ ] A check that could not be made is recorded as neither passed nor failed, with a
      diagnostic naming the reason — not as a pass
- [ ] Every new check has a stable identifier and a `detail` saying what it was
- [ ] There is a test for the dimension, and one in `integration-tests/` if the change
      is visible in a run's output

## If this touches parsing or an artifact

- [ ] A malformed document produces the right failure class, not a panic
- [ ] `fuzz/` covers the boundary, or it is said here why it does not

## Anything a reviewer should know

<!--
Known limitations, a follow-up you deliberately left out, a trade-off you were unsure
about, or a case you could not test.
-->
