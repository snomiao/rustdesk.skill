#!/usr/bin/env bash
# Install this repo's git hooks (the PUBLIC-repo secret scanner).
set -euo pipefail
cd "$(dirname "$0")/.."
chmod +x .githooks/* 2>/dev/null || true
git config core.hooksPath .githooks
echo "✓ core.hooksPath = .githooks  (pre-commit secret scan enabled)"
echo "  bypass once with: git commit --no-verify"
echo "  add private patterns (ERE, one per line) to .secret-denylist (gitignored)"
