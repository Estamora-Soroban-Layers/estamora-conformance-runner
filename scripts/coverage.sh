#!/usr/bin/env bash
#
# Measure line coverage over the code this project ships, and fail below a floor.
#
# What is measured, and what is not, stated explicitly because a coverage figure is only
# meaningful next to its denominator:
#
#   crates/                   measured. This is the runner: the thing a reviewer is judging.
#   fixtures/contracts/       not measured. These are deliberately-defective Soroban contracts
#                             used as *inputs* -- one of them is the free-mint bug the worked
#                             example is built around. They are subject matter, not tooling, and
#                             their coverage says nothing about the runner's correctness.
#   integration-tests/        not measured. Test code.
#   benches/                  not measured. Benchmark harnesses.
#
# The floor defaults to 70% and is enforced by `cargo llvm-cov` itself, so this script exits
# non-zero when the number drops -- which is the point of running it in CI. Pass a different
# floor as the first argument when investigating a regression.
#
# `--no-fail-fast` keeps every test suite running when one fails, so a single broken suite does
# not hide the coverage of the rest of the workspace.
set -euo pipefail

floor="${1:-70}"

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
  echo "cargo-llvm-cov is not installed. Install it with:" >&2
  echo >&2
  echo "  rustup component add llvm-tools-preview" >&2
  echo "  cargo install cargo-llvm-cov --locked" >&2
  exit 1
fi

cargo llvm-cov \
  --workspace \
  --no-fail-fast \
  --summary-only \
  --ignore-filename-regex '(^|/)(fixtures|integration-tests|benches)/' \
  --fail-under-lines "${floor}"
