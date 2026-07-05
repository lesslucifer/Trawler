#!/usr/bin/env bash
# Stop: refuse to end the turn while any ready governed doc under docs/ is red.
# Backstop for docs edited around the file tools (shell, scripts).
set -u

cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0

files=$(git ls-files --cached --others --exclude-standard -- 'docs/*.html' 'docs/**/*.html' 2>/dev/null | sort -u)
[ -n "$files" ] || exit 0

# shellcheck disable=SC2086 # word-splitting the newline-separated list is intended
out=$(${XTASK_BIN:-cargo xtask} verify-doc $files 2>&1)
rc=$?
if [ "$rc" -eq 1 ]; then
  printf 'A ready governed doc is red - fix it before ending the turn (see docs/work-planning.html):\n%s\n' "$out" >&2
  exit 2
fi
exit 0
