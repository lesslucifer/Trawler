#!/usr/bin/env bash
# PostToolUse(Write|Edit): verify a governed doc the moment it is written.
# Red -> exit 2 with the problem blocks on stderr (fed back to the session).
# The write itself is never blocked; drafts and ungoverned docs stay silent.
set -u

input=$(cat)
path=$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty' 2>/dev/null)
case "$path" in
  *.html) ;;
  *) exit 0 ;;
esac
[ -f "$path" ] || exit 0

cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0
out=$(${XTASK_BIN:-cargo xtask} verify-doc "$path" 2>&1)
rc=$?
if [ "$rc" -eq 1 ]; then
  printf '%s\n' "$out" >&2
  exit 2
fi
exit 0
