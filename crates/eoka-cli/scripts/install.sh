#!/bin/sh
# Install the current macOS Apple Silicon eoka CLI release.
#
# Pinned to v0.1.1 deliberately, not because it's stale: CI (see
# .github/workflows/publish.yml) only publishes eoka-cli to crates.io and
# tags the release now — it no longer builds/uploads a macOS binary asset
# (unlike eoka-server, which still has release-eoka-server.yml for that).
# v0.1.1 is the newest eoka-cli release that actually has a downloadable
# aarch64-apple-darwin tarball attached; bumping this to a newer tag will
# 404. Until eoka-cli gets its own binary-release workflow, `cargo install
# eoka-cli` is the only way to get a current version.
set -eu

version="v0.1.1"
asset="eoka-cli-${version}-aarch64-apple-darwin.tar.gz"
checksum="eb9cbc47e65fd5a5bcd5aace35b75ed661b68807c9f8e9f01e69b0e58f4f75cf"
install_dir="${EOKA_INSTALL_DIR:-$HOME/.local/bin}"
url="https://github.com/shrimp-software/eoka-tools/releases/download/eoka-cli-${version}/${asset}"

if [ "$(uname -s)" != "Darwin" ] || [ "$(uname -m)" != "arm64" ]; then
    echo "This release installer supports macOS Apple Silicon. Use: cargo install eoka-cli" >&2
    exit 1
fi

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT HUP INT TERM

curl --fail --location --silent --show-error "$url" --output "$workdir/$asset"
printf '%s  %s\n' "$checksum" "$workdir/$asset" | shasum -a 256 --check --status
tar -xzf "$workdir/$asset" -C "$workdir" --strip-components=1 '*/eoka'

mkdir -p "$install_dir"
install -m 755 "$workdir/eoka" "$install_dir/eoka"
echo "Installed eoka to $install_dir/eoka"
