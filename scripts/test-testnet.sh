#!/usr/bin/env bash
#
# The network-backed tests, which are opt-in and separate from the default suite.
#
# Three reasons it is a separate script rather than a feature flag on `cargo test`:
#
#   * `cargo test --workspace` must not depend on a node being up, or a flaky
#     connection becomes a regression and people learn to ignore the suite;
#   * a network failure and a non-conformant contract have nothing in common, and a
#     single command that produces both would blur the distinction the exit codes
#     exist to keep clear;
#   * a test that needs a funded account must not be a prerequisite for checking a
#     rule about event cardinality.
#
# Reaching a network is opt-in behind ESTAMORA_TESTNET_ENABLED, and the tests themselves
# check it as well as this script. That is not redundancy: `cargo test --workspace --
# --ignored` is what CI runs to reach the cross-repository tests, so a test that was only
# `#[ignore]`d would read a public node on every push. A test set that either measured
# something or said why it could not is the point, and a suite that reports success because
# it ran nothing is worse than one that fails.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

say() { printf '%s\n' "$*" >&2; }

if [ "${ESTAMORA_TESTNET_ENABLED:-0}" != "1" ]; then
    say "testnet: skipped. Set ESTAMORA_TESTNET_ENABLED=1 to run these tests."
    exit 0
fi

SPEC="${ESTAMORA_SPEC_REPO:-$ROOT/../estamora-conformance-spec}"
if [ ! -d "$SPEC/profiles/sep-41/1.0" ]; then
    say "testnet: no specification checkout at $SPEC; set ESTAMORA_SPEC_REPO"
    exit 4
fi

if [ -z "${ESTAMORA_TESTNET_CONTRACT:-}" ]; then
    say "testnet: ESTAMORA_TESTNET_CONTRACT names the deployed contract to measure;"
    say "testnet: without it the tests that need one will report as skipped."
fi

say "testnet: running the network-backed suite (network: ${ESTAMORA_TESTNET_NETWORK:-testnet})"

# `--ignored` rather than a separate target: the network tests live beside the others
# and are marked `#[ignore]`, so their absence from the default run is visible in the
# test output rather than hidden in a second suite nobody runs.
exec cargo test --workspace -- --ignored

# The classification properties a network run must never break are asserted in
# `integration-tests/testnet/`, which needs no network and therefore runs in the
# default suite: a network failure is an environment failure, it exits 4, and it can
# never share an exit code with a non-conformant contract.
