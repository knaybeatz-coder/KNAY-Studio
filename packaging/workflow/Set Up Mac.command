#!/bin/bash
set -euo pipefail
SHARE="${1:-/Volumes/Home-Shared/Storage/External-HD-Large-Files/Lemon Studio}"
DEST="$HOME/Documents/Lemon Studio Source"
REPO="$SHARE/Lemon Studio.git"
[[ -d "$REPO" ]] || { echo 'Mount smb://192.168.178.132/Home-Shared in Finder first.'; exit 1; }
if [[ ! -e "$DEST" ]]; then
  git clone --origin home "$REPO" "$DEST"
elif [[ -d "$DEST/.git" ]]; then
  [[ "$(git -C "$DEST" remote get-url home)" == "$REPO" ]] || { echo 'Existing checkout uses a different remote; preserve and reconcile it.'; exit 1; }
  [[ -z "$(git -C "$DEST" status --porcelain)" ]] || { echo 'Existing local changes must be committed/reconciled first.'; exit 1; }
  git -C "$DEST" pull --ff-only home main
else
  echo 'Destination already exists; preserved without changes.'; exit 1
fi
git -C "$DEST" config pull.ff only
git -C "$DEST" config push.default simple
git -C "$DEST" config user.name 'Michael Lane'
git -C "$DEST" config user.email 'michael-lane@localhost'
mkdir -p "$HOME/Desktop/Lemon Studio Workflow"
cp "$DEST/packaging/workflow/WORKFLOW.md" "$HOME/Desktop/Lemon Studio Workflow/START HERE.md"
printf '#!/bin/bash\nexec bash %q\n' "$DEST/packaging/workflow/Build and Publish.command" > "$HOME/Desktop/Lemon Studio Workflow/Build and Publish.command"
printf '#!/bin/bash\nexec bash %q\n' "$DEST/packaging/workflow/Connect Publishing.command" > "$HOME/Desktop/Lemon Studio Workflow/Connect Publishing.command"
chmod +x "$HOME/Desktop/Lemon Studio Workflow/"*.command
echo 'Local Mac checkout ready. Read AGENTS.md. Run Connect Publishing once, then Build and Publish.'
