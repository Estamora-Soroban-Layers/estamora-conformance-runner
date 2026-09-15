#!/usr/bin/env bash
#
# Install the `estamora` binary from this checkout.
#
# Source-first on purpose: there is no prebuilt binary to fetch and no network step
# that could install something other than the revision you are standing in. What
# gets installed is what you can read, and `estamora --version` identifies it.
#
# The workspace's Rust toolchain is pinned in `rust-toolchain.toml`, so `rustup`
# selects it here rather than whatever the machine happens to have. A conformance
# result is only meaningful if the tooling that produced it is identified, which is
# why the toolchain is part of the repository rather than a prerequisite in a README.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE="estamora-cli"
BIN="estamora"

say() { printf '%s\n' "$*" >&2; }
die() { say "install: $*"; exit 1; }

command -v cargo >/dev/null 2>&1 || die "cargo is not on PATH; install Rust from https://rustup.rs first"
command -v rustup >/dev/null 2>&1 || say "install: rustup is not on PATH; cargo will use whatever toolchain it finds"

cd "$ROOT"

say "install: building $CRATE from $ROOT"

# `--locked` is not a convenience. The lock file is committed because an unpinned
# dependency tree would let a conformance verdict change without a commit, which is
# the drift this project exists to make visible. An install that silently re-resolved
# it would be installing a different tool from the one the repository describes.
if ! cargo install --locked --path "crates/$CRATE" "$@"; then
    die "the build failed; run 'cargo build -p $CRATE' to see the errors in full"
fi

if ! command -v "$BIN" >/dev/null 2>&1; then
    say "install: $BIN was built but is not on PATH"
    say "install: add \"\$HOME/.cargo/bin\" to PATH, or pass --root to choose where it goes"
    exit 0
fi

say "install: $("$BIN" --version) is ready"
say "install: point it at a specification checkout with --spec or \$ESTAMORA_SPEC_REPO"
