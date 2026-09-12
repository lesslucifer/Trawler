#!/usr/bin/env bash
# Stop: refuse to end the turn while any doc under docs/ is red - a ready governed
# one on its checklists, any doc on its links.
# Backstop for docs edited around the file tools (shell, scripts).
set -u

root="${TRAWLER_HOOK_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null || echo "${CLAUDE_PROJECT_DIR:-.}")}"
cd "$root" || exit 0

listing=$(git ls-files --cached --others --exclude-standard -- 'docs/*.html' 'docs/**/*.html' 2>/dev/null | sort -u)
[ -n "$listing" ] || exit 0

files=()
while IFS= read -r path; do
  [ -n "$path" ] || continue
  files+=("$path")
done <<<"$listing"

out=$(${XTASK_BIN:-cargo xtask} verify-doc "${files[@]}" 2>&1)
rc=$?
if [ "$rc" -eq 1 ]; then
  printf 'A doc is red - fix it before ending the turn (see docs/work-planning.html):\n%s\n' "$out" >&2
  exit 2
fi
exit 0
