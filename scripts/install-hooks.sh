#!/bin/sh
#
# Install git hooks for lclq development
#
# Usage: ./scripts/install-hooks.sh
#

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOOKS_DIR="$REPO_ROOT/.git/hooks"

echo "Installing git hooks for lclq..."

# Ensure we're in a git repository
if [ ! -d "$REPO_ROOT/.git" ]; then
    echo "ERROR: Not a git repository. Please run from the repository root."
    exit 1
fi

# Install pre-commit hook
echo "  Installing pre-commit hook..."
cp "$SCRIPT_DIR/pre-commit" "$HOOKS_DIR/pre-commit"
chmod +x "$HOOKS_DIR/pre-commit"

echo ""
echo "Git hooks installed successfully!"
echo ""
echo "Installed hooks:"
echo "  - pre-commit: Runs rustfmt on staged Rust files"
echo ""
echo "To uninstall, run: rm .git/hooks/pre-commit"
