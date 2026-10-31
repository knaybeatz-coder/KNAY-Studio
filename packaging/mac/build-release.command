#!/bin/bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
[[ "$(uname -s)" == Darwin ]] || { echo 'Build this package on the MacBook.'; exit 1; }
[[ "$(sysctl -in sysctl.proc_translated 2>/dev/null || true)" != 1 ]] || { echo 'Use a native terminal, not Rosetta.'; exit 1; }
export PATH="$HOME/.cargo/bin:$PATH"
export OPENSTUDIO_NO_PLUGIN_BUILD=1
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/lemon-studio-remote-target}"
unset CARGO_BUILD_TARGET
xcode-select -p >/dev/null
cd "$ROOT"
# LAME's autotools build rejects spaces in its source directory as well.
mkdir -p "$HOME/.cache/lemon-studio-build-sources"
BUILD_SOURCE="$(mktemp -d "$HOME/.cache/lemon-studio-build-sources/release.XXXXXX")"
git archive HEAD | tar -x -C "$BUILD_SOURCE"
cd "$BUILD_SOURCE"
cargo build --locked --release -p os-app
cargo run --locked --release -p bundle-app
APP="$CARGO_TARGET_DIR/bundled/Lemon Studio.app"
codesign --verify --deep --strict "$APP"
python3 "$ROOT/packaging/mac/package-release.py" "$APP"
