#!/usr/bin/env bash
#
# Builds the fixture contract that is measured as a compiled artifact, and refreshes the
# committed copy of it.
#
#   $ ./scripts/build-fixture-wasm.sh
#
# # Why a compiled fixture exists at all
#
# The runner has three targets, and two of them were tested: an in-repository fixture is
# registered from its Rust type, and a deployed contract resolves over RPC. The third —
# a `.wasm` file on disk — could not be, because there was no artifact to test it with.
# `fixtures/contracts/conformance-token` is a workspace member, and the workspace enables
# `soroban-sdk`'s `testutils` for every crate in it, which does not compile for
# WebAssembly. So no contract in that workspace could be built for Wasm at all.
#
# `fixtures/contracts/measurable-token` therefore declares its own `[workspace]`, builds
# for `wasm32v1-none`, and its artifact is committed under `fixtures/wasm/`. The default
# test suite measures it, so interface inspection from a real `contractspecv0` section
# and deployment from real bytes are covered without a build step and without a network.
#
# # Reproducibility
#
# CI runs this script and fails if the working tree changed, so the committed artifact is
# either the one this source produces or a red job. The lock file beside the contract is
# committed for the same reason: `--locked` means a build resolves the versions the
# committed artifact was built from, rather than whatever is newest that day.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_dir="$root/fixtures/contracts/measurable-token"
artifact_name="estamora_fixture_measurable_token.wasm"
built="$source_dir/target/wasm32v1-none/release/$artifact_name"
committed="$root/fixtures/wasm/measurable-token.wasm"

if ! command -v rustup >/dev/null 2>&1; then
    echo "rustup is required: the contract is built for a target this toolchain may not have" >&2
    exit 1
fi

if ! rustup target list --installed | grep -qx 'wasm32v1-none'; then
    echo "the wasm32v1-none target is not installed; run: rustup target add wasm32v1-none" >&2
    exit 1
fi

# Built from inside its own directory so that the contract's own workspace is the one
# resolved. Building it from the repository root would resolve the runner's workspace,
# which is the build that cannot produce a contract.
(cd "$source_dir" && cargo build --target wasm32v1-none --release --locked)

mkdir -p "$(dirname "$committed")"
cp "$built" "$committed"

echo "built  $built"
echo "copied $committed"
