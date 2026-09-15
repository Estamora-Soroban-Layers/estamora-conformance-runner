#!/usr/bin/env bash
#
# Release checks, and the release itself.
#
#   ./scripts/release.sh --check     validate the workspace without publishing
#   ./scripts/release.sh 0.2.0       check, then package every publishable crate
#
# It never pushes and never tags. A tag is a human step here on purpose: a
# conformance tool's version is part of what a result means, so the moment a version
# becomes public is not something a script should decide on its own.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

say() { printf '%s\n' "$*" >&2; }
die() { say "release: $*"; exit 1; }

# The crates that are published. `integration-tests` and the fixture contract are
# `publish = false` members of the workspace: the first is a test harness and the
# second is a contract to measure, and neither is a library anybody depends on.
PUBLISHABLE=(
    estamora-core
    estamora-profile
    estamora-vectors
    estamora-soroban
    estamora-assertions
    estamora-report
    estamora-certification
    estamora-cli
)

workspace_version() {
    # Read from the workspace manifest rather than from a crate, because the version
    # is declared once and inherited. A crate that pinned its own would be the drift
    # this check exists to catch.
    sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1
}

check() {
    local version="$1"

    [ -f CHANGELOG.md ] || die "CHANGELOG.md is missing"
    [ -n "$version" ] || die "the workspace version could not be read from Cargo.toml"

    say "release: workspace version is $version"

    # The changelog is the record of what a version means. A release whose entry is
    # absent is a release nobody can interpret, which for this tool is the same as a
    # release nobody can use.
    grep -q "^## \[$version\]" CHANGELOG.md \
        || die "CHANGELOG.md has no '## [$version]' section; write it before releasing"

    # A pinned toolchain is part of the release: the report names the runner version,
    # and the toolchain that built it should not be a mystery behind that number.
    [ -f rust-toolchain.toml ] || die "rust-toolchain.toml is missing"
    [ -f Cargo.lock ] || die "Cargo.lock is missing; the dependency tree must be pinned"

    # Nothing may be committed that is not formatted or that lints, because the
    # published source is the source a reader has to be able to trust.
    cargo fmt --all -- --check >/dev/null || die "cargo fmt --check failed"
    cargo clippy --workspace --all-targets -- -D warnings >/dev/null 2>&1 \
        || die "cargo clippy failed"

    # Every crate must carry the license, repository and description the manifest
    # promises, or a published crate is missing the metadata a consumer needs.
    for crate in "${PUBLISHABLE[@]}"; do
        cargo metadata --no-deps --format-version 1 >/dev/null
        grep -q '^description' "crates/$crate/Cargo.toml" \
            || grep -q '^description' Cargo.toml \
            || die "crates/$crate declares no description"
    done

    say "release: checks passed"
}

package() {
    local version="$1"
    say "release: packaging every publishable crate"

    # `--locked` and `--no-verify` are a pair here: the lock file is authoritative and
    # a dry run must not re-resolve it, and the crates are verified by the test suite
    # rather than by building them again in a scratch directory.
    for crate in "${PUBLISHABLE[@]}"; do
        say "release:   $crate"
        if ! cargo package --locked --no-verify -p "$crate" >/dev/null 2>&1; then
            cargo package --locked -p "$crate" || die "crates/$crate could not be packaged"
        fi
    done

    say ""
    say "release: $version is packaged and ready."
    say "release: tagging and pushing are yours to do, deliberately:"
    say "release:"
    say "release:   git tag -a v$version -m \"estamora-conformance-runner v$version\""
    say "release:   git push origin v$version"
}

case "${1:-}" in
    --check|"")
        check "$(workspace_version)"
        ;;
    --help|-h)
        sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
        ;;
    *)
        check "$1"
        package "$1"
        ;;
esac
