#!/usr/bin/env bash
set -euo pipefail

UPSTREAM_REMOTE="upstream"
UPSTREAM_BRANCH="master"
LOCAL_BRANCH="master"

# Ensure upstream remote exists
if ! git remote get-url "$UPSTREAM_REMOTE" &>/dev/null; then
    echo "Adding upstream remote..."
    git remote add "$UPSTREAM_REMOTE" https://github.com/foundry-rs/foundry.git
fi

echo "Fetching upstream..."
git fetch "$UPSTREAM_REMOTE"

echo "Merging $UPSTREAM_REMOTE/$UPSTREAM_BRANCH into $LOCAL_BRANCH..."
git merge "$UPSTREAM_REMOTE/$UPSTREAM_BRANCH" --no-edit

echo ""
echo "Merge complete. If there are conflicts:"
echo "  1. Resolve conflicts in modified files (see FORK.md for list)"
echo "  2. git add <resolved files>"
echo "  3. git merge --continue"
echo ""
echo "Run 'cargo build -p cast --features cobo-mpc,remote-signer,batch-ops' to verify."
