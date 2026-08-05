#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if grep -q '^\[patch.crates-io\]' Cargo.toml; then
  echo "Remove checked-in [patch.crates-io] overrides before release."
  exit 1
fi

cleanup() {
  if [[ -n "${TMP_CARGO_HOME:-}" && -d "${TMP_CARGO_HOME}" ]]; then
    find "${TMP_CARGO_HOME}" -mindepth 1 -delete
    rmdir "${TMP_CARGO_HOME}"
  fi
}
trap cleanup EXIT

if [[ "${1:-}" == "--with-local-tack" ]]; then
  tack_root="${TACK_RS_PATH:-../tack-rs}"
  tack_root="$(cd "${tack_root}" && pwd)"
  base_cargo_home="${CARGO_HOME:-${HOME}/.cargo}"
  TMP_CARGO_HOME="$(mktemp -d)"

  if [[ -d "${base_cargo_home}/registry" ]]; then
    ln -s "${base_cargo_home}/registry" "${TMP_CARGO_HOME}/registry"
  fi
  if [[ -d "${base_cargo_home}/git" ]]; then
    ln -s "${base_cargo_home}/git" "${TMP_CARGO_HOME}/git"
  fi

  cat >"${TMP_CARGO_HOME}/config.toml" <<EOF
[patch.crates-io]
tack-core = { path = "${tack_root}/crates/tack-core" }
tack-runtime-quickjs = { path = "${tack_root}/crates/tack-runtime-quickjs" }
EOF

  export CARGO_HOME="${TMP_CARGO_HOME}"
fi

cargo fmt --check
cargo check --workspace --all-features
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings

# eoka-protocol is the first new Eoka crate in the publish chain. Full
# workspace packaging succeeds after Tack 0.0.2 and eoka-protocol 0.1.0 are
# visible in crates.io.
cargo package -p eoka-protocol --allow-dirty
cargo package --workspace --allow-dirty --no-verify

cat <<'MSG'

Release check passed.

Publish order:
  1. publish tack-rs 0.0.2 crates first
  2. cargo publish -p eoka-protocol
  3. cargo publish -p eoka-sdk
  4. cargo publish -p eoka-mcp
  5. cargo publish -p eoka-tack
  6. cargo publish -p eoka-cli
MSG
