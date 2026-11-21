#!/usr/bin/env bash
# Build the release packages: a static (musl) wysebee-agent for x86_64 and aarch64, each in
# dist/wysebee-agent-linux-<arch>.tar.gz, with dist/SHA256SUMS and dist/install.sh beside
# them — exactly the files a GitHub release holds (docs/distribute.md).
#
#   ./scripts/build_linux.sh                 both architectures
#   ./scripts/build_linux.sh x86_64          one
#
# On Linux with the musl targets installed (`rustup target add x86_64-unknown-linux-musl
# aarch64-unknown-linux-musl`, and musl-tools) it builds directly for this machine's
# architecture; anywhere else (a Mac) it builds in Docker (rust:alpine), one container per
# architecture (the other one emulated, so slower: on Apple silicon, x86_64 needs Docker
# Desktop's "Use Rosetta for x86_64/amd64 emulation"). The release workflow
# (.github/workflows/release.yml) builds each on its own runner.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
ARCHES=("${@:-x86_64 aarch64}")
# shellcheck disable=SC2206
ARCHES=(${ARCHES[*]})
VERSION="$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)"
mkdir -p dist

build_one() {
  local arch="$1" bin
  if [[ "$(uname -s)" == "Linux" && "$(uname -m)" == "$arch" ]] && rustup target list --installed 2>/dev/null | grep -q "^$arch-unknown-linux-musl$"; then
    cargo build --release --locked -p agent-cli --target "$arch-unknown-linux-musl"
    bin="target/$arch-unknown-linux-musl/release/wysebee-agent"
  else
    command -v docker >/dev/null || { echo "Docker is needed to build for $arch here." >&2; exit 1; }
    local platform=amd64
    [[ "$arch" == "aarch64" ]] && platform=arm64
    docker run --rm --platform "linux/$platform" \
      -v "$REPO_ROOT":/src -w /src \
      -v "wysebee-agent-cargo-$arch":/usr/local/cargo/registry \
      -e CARGO_TARGET_DIR="/src/target/docker-$arch" \
      rust:1-alpine sh -c "apk add --no-cache musl-dev >/dev/null && cargo build --release --locked -p agent-cli" \
      || { echo "The $arch build failed in Docker. If it said 'exec format error', Docker cannot run $platform containers here: turn on its x86_64/amd64 emulation, or let the release workflow build it." >&2; exit 1; }
    bin="target/docker-$arch/release/wysebee-agent"
  fi
  file "$bin" 2>/dev/null | grep -q "statically linked\|static-pie" || echo "warning: $bin may not be static: $(file "$bin" 2>/dev/null)" >&2
  local stage
  stage="$(mktemp -d)"
  cp "$bin" "$stage/wysebee-agent"
  cp README.md "$stage/README.md"
  cp LICENSE "$stage/LICENSE"
  python3 scripts/third_party_licenses.py "$arch-unknown-linux-musl" "$stage/THIRD_PARTY_LICENSES.txt"
  echo "$VERSION" > "$stage/VERSION"
  # No macOS metadata in the package (GNU tar warns of it).
  if [[ "$(uname -s)" == "Darwin" ]]; then
    COPYFILE_DISABLE=1 tar --no-xattrs -czf "dist/wysebee-agent-linux-$arch.tar.gz" -C "$stage" wysebee-agent README.md LICENSE THIRD_PARTY_LICENSES.txt VERSION
  else
    tar -czf "dist/wysebee-agent-linux-$arch.tar.gz" -C "$stage" wysebee-agent README.md LICENSE THIRD_PARTY_LICENSES.txt VERSION
  fi
  rm -rf "$stage"
  echo "== dist/wysebee-agent-linux-$arch.tar.gz ($VERSION)"
}

for arch in "${ARCHES[@]}"; do
  case "$arch" in x86_64|aarch64) build_one "$arch" ;; *) echo "No build for $arch." >&2; exit 1 ;; esac
done
cp scripts/install.sh dist/install.sh
( cd dist && shasum -a 256 wysebee-agent-linux-*.tar.gz install.sh 2>/dev/null || sha256sum wysebee-agent-linux-*.tar.gz install.sh ) > dist/SHA256SUMS.tmp
mv dist/SHA256SUMS.tmp dist/SHA256SUMS
cat dist/SHA256SUMS
