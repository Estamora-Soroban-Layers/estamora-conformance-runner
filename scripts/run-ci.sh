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
cd "$ROOT"

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

run "release checks" \
    ./scripts/release.sh --check

printf '\n'
if [ "$FAILED" -eq 0 ]; then
    printf '\033[32mall checks passed\033[0m\n'
else
    printf '\033[31mone or more checks failed\033[0m\n'
fi
exit "$FAILED"
