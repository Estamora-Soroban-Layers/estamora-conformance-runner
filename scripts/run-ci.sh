#!/usr/bin/env bash
#
# Everything CI runs, in one command, so that a red job can be reproduced locally
# without pushing.
#
# The order is deliberate and stops at the first failure: formatting and lints are
# seconds and catch the cheapest mistakes, the build is next, and the tests are last
# because they are the slowest to fail informatively. A contributor who has a typo
# should not wait for a full test run to hear about it.
#
# Two facts about this script are worth stating, because they are what make it
# trustworthy as a local substitute for CI:
#
#   * It fails loudly if the cross-repository tests are skipped, unless they were
#     explicitly excused. A suite that quietly runs less than CI does is worse than no
#     local script at all — it produces confidence that was not earned.
#   * Nothing here touches a network. `cargo test --workspace` needs no node, and the
#     network tests are gated behind `test-testnet.sh`.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# `set -e` is deliberately off in this script — it has to run every step and report at
# the end rather than stop at the first failure — so the one command whose failure would
# invalidate everything after it checks itself.
cd "$ROOT" || exit 1

# `--spec` is passed explicitly below rather than left to the environment, so that the
# profile this repository validates is the one in the repository.
SPEC="${ESTAMORA_SPEC_REPO:-$ROOT/../estamora-conformance-spec}"

FAILED=0
step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }
fail() { printf '\033[31mfailed: %s\033[0m\n' "$*"; FAILED=1; }

run() {
    local name="$1"; shift
    step "$name"
    if ! "$@"; then
        fail "$name"
    fi
}

# `--allow-skip-spec` exists for a contributor working on the parser, who has no
# reason to check out the specification repository and no business being told they
# broke something. It still prints that the cross-repository half did not run.
ALLOW_SKIP_SPEC=0
for argument in "$@"; do
    case "$argument" in
        --allow-skip-spec) ALLOW_SKIP_SPEC=1 ;;
        *) printf 'usage: %s [--allow-skip-spec]\n' "$0" >&2; exit 64 ;;
    esac
done

run "cargo fmt --check" \
    cargo fmt --all -- --check

run "cargo clippy" \
    cargo clippy --workspace --all-targets -- -D warnings

run "cargo build" \
    cargo build --workspace --all-targets

run "cargo test" \
    cargo test --workspace

# The fixture bundle this repository measures its own contracts with. Validating it
# here is what catches an edit that made a requirement unreachable without changing
# any Rust: the loader refuses a bundle whose cross-references do not resolve, and
# nothing else in this script reads it.
run "validate the in-repository profile" \
    cargo run --quiet -p estamora-cli -- validate --spec fixtures --profile conformance-token@1.0

# The end-to-end verdicts, run through the binary rather than the library, so that the
# argument parsing and the exit codes are exercised too.
run "conformance run against the reference fixture" \
    cargo run --quiet -p estamora-cli -- run --spec fixtures --profile conformance-token@1.0 --contract fixture:none

if [ -d "$SPEC/profiles/sep-41/1.0" ]; then
    run "the SEP-41 profile resolves" \
        cargo run --quiet -p estamora-cli -- --spec "$SPEC" profile --profile sep-41@1.0
    run "the SEP-41 profile validates" \
        cargo run --quiet -p estamora-cli -- --spec "$SPEC" validate --profile sep-41@1.0
else
    step "cross-repository checks"
    if [ "$ALLOW_SKIP_SPEC" -eq 1 ]; then
        printf 'skipped: no specification checkout at %s (--allow-skip-spec)\n' "$SPEC"
    else
        fail "no specification checkout at $SPEC; pass --allow-skip-spec if that is intended"
    fi
fi

# The fixture contract that is compiled for WebAssembly has its own workspace — outside
# the runner's, on purpose — so none of the checks above reaches it. It is checked here
# for the same reason it is checked in CI: a contract nobody compiled is a contract
# nobody has verified, and the artifact the end-to-end suite measures comes from it.
run "the compiled fixture contract: formatting" \
    cargo fmt --manifest-path fixtures/contracts/measurable-token/Cargo.toml -- --check

run "the compiled fixture contract: lints" \
    cargo clippy --manifest-path fixtures/contracts/measurable-token/Cargo.toml \
    --all-targets -- -D warnings

run "the compiled fixture contract: tests" \
    cargo test --manifest-path fixtures/contracts/measurable-token/Cargo.toml

# And the committed artifact is rebuilt, because a committed binary whose source has moved
# on passes every test while measuring a compilation the checkout no longer describes.
step "the committed fixture artifact is current"
if ./scripts/build-fixture-wasm.sh >/dev/null \
    && git diff --quiet -- fixtures/wasm fixtures/contracts/measurable-token/Cargo.lock; then
    printf 'the artifact matches the source beside it\n'
else
    fail "fixtures/wasm is stale; run ./scripts/build-fixture-wasm.sh and commit the result"
fi

run "release checks" \
    ./scripts/release.sh --check

# The plan the crates.io publish commands are run in. `cargo publish` is a human command,
# so the sequence is a comment in the release workflow and this is the only thing that
# reads it: it derives the crates that can actually be delivered from `cargo metadata` and
# fails when the comment disagrees. The second step is what shows the first one can fail —
# a checker nobody has seen fail is a checker nobody can rely on.
run "the publish plan" \
    bash -c 'cargo metadata --no-deps --format-version 1 | python3 scripts/check-publish-plan.py'

run "the publish-plan checker" \
    ./scripts/test-check-publish-plan.sh

printf '\n'
if [ "$FAILED" -eq 0 ]; then
    printf '\033[32mall checks passed\033[0m\n'
else
    printf '\033[31mone or more checks failed\033[0m\n'
fi
exit "$FAILED"
