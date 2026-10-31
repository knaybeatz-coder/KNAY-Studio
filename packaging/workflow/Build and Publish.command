#!/bin/bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TOOLS="$HOME/.lemon-studio-release/tools"
export PATH="$HOME/.cargo/bin:$PATH"
if [ ! -x "$TOOLS/bin/python" ]; then
  python3 -m venv "$TOOLS"
  "$TOOLS/bin/python" -m pip install -r "$ROOT/packaging/workflow/requirements.txt"
fi
"$TOOLS/bin/python" "$ROOT/packaging/workflow/release.py" build-publish
