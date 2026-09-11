#!/usr/bin/env bash
# PostToolUse(Write|Edit|apply_patch): verify a governed doc the moment it is written.
# Red -> exit 2 with the problem blocks on stderr (fed back to the session).
# The write itself is never blocked; drafts and ungoverned docs stay silent.
set -u

input=$(cat)

# Claude names the edited file; Codex hands over the apply_patch text, whose
# *** Add/Update/Delete File: and *** Move to: headers name the targets.
paths=$(printf '%s' "$input" | jq -r '
  if (.tool_input.file_path? // "") != "" then .tool_input.file_path
  else (.tool_input.command? // "")
    | split("\n")[]
    | select(test("^\\*\\*\\* (Add|Update|Delete) File: |^\\*\\*\\* Move to: "))
    | sub("^\\*\\*\\* (Add|Update|Delete) File: |^\\*\\*\\* Move to: "; "")
  end' 2>/dev/null) || exit 0

# Targets resolve against the session's cwd - where the harness named them - and
# travel as absolute paths, so the hook may still run verify-doc from the root.
docs=()
while IFS= read -r path; do
  case "$path" in
    *.html) ;;
    *) continue ;;
  esac
  [ -f "$path" ] || continue
  case "$path" in
    /*) ;;
    *) path="$PWD/$path" ;;
  esac
  docs+=("$path")
done <<<"$paths"
[ "${#docs[@]}" -gt 0 ] || exit 0

root="${TRAWLER_HOOK_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null || echo "${CLAUDE_PROJECT_DIR:-.}")}"
cd "$root" || exit 0

out=$(${XTASK_BIN:-cargo xtask} verify-doc "${docs[@]}" 2>&1)
rc=$?
[ "$rc" -eq 1 ] || exit 0
printf '%s\n' "$out" >&2
exit 2
