#!/usr/bin/env bash
# Everything must pass, every time: the tests, clippy with no warning, and 100% of
# agent-core's lines covered. Run it before every commit.
#
#   ./scripts/check.sh            here (Linux; a Mac runs the same tests)
#   ./scripts/check.sh --docker   in a Linux container, from a Mac
#
# Nothing is excluded from the coverage rule: cover new code with a real test, never with an
# exclusion. agent-cli (the command line) is built, linted and its own tests run, but not
# measured: it is the real Options, systemd and printing, over the core.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"   # a Mac's Homebrew rustup is keg-only
cd "$REPO_ROOT"

if [[ "${1:-}" == "--docker" ]]; then
  # As an ordinary account, as on a real machine and in CI: root may delete what the tests
  # lock against the agent.
  exec docker run --rm -v "$REPO_ROOT":/src -w /src -v wysebee-agent-cargo-check:/cargo rust:1 bash -c '
    set -e
    rustup component add clippy llvm-tools-preview >/dev/null 2>&1
    useradd -m tester && mkdir -p /cargo && chown tester /cargo
    su tester -c "export PATH=/usr/local/cargo/bin:\$PATH CARGO_HOME=/cargo RUSTUP_HOME=/usr/local/rustup CARGO_TARGET_DIR=/cargo/target CARGO_BUILD_JOBS=2
      (cargo llvm-cov --version >/dev/null 2>&1 || cargo install cargo-llvm-cov --locked >/dev/null) && ./scripts/check.sh"'
fi

if ! cargo llvm-cov --version >/dev/null 2>&1; then
  echo "cargo-llvm-cov is needed: cargo install cargo-llvm-cov" >&2
  exit 1
fi

echo "== clippy"
cargo clippy --workspace --all-targets -- -D warnings

echo "== tests"
cargo test --workspace

echo "== coverage (agent-core, 100% of lines)"
cargo llvm-cov -p agent-core --show-missing-lines --fail-uncovered-lines 0
# (--fail-uncovered-lines, not --fail-under-lines: the TOTAL row counts each test binary's
# own copy of a function, so it stays under 100% even when every line is run.)

echo "== the installer (sh -n)"
sh -n scripts/install.sh

echo "All green."
