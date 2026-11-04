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
# A stable, locked, pipeline-owned worktree preserves Cargo build-script paths.
BUILD_SOURCE="$(python3 "$ROOT/packaging/workflow/mac-worktree.py" "$ROOT" "${LEMON_MAC_ARCH:-$(uname -m)}")"
cd "$BUILD_SOURCE"
export LEMON_RELEASE=1
: "${LEMON_MASTER_BUNDLE:?Set LEMON_MASTER_BUNDLE to the native Mac engine archive}"
case "${LEMON_MAC_ARCH:-$(uname -m)}" in
 arm64) export LEMON_APP_TARGET=aarch64-apple-darwin ;;
 x86_64) export LEMON_APP_TARGET=x86_64-apple-darwin ;;
 *) echo 'Unsupported Mac architecture'; exit 1 ;;
esac
export MACOSX_DEPLOYMENT_TARGET=12.0
cargo build --locked --release -p os-app --target "$LEMON_APP_TARGET"
cargo run --locked --release -p bundle-app
APP="$CARGO_TARGET_DIR/bundled/Lemon Studio.app"
codesign --verify --deep --strict "$APP"
python3 "$ROOT/packaging/mac/package-release.py" "$APP"
