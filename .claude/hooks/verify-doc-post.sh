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

# Targets stay relative to the session's cwd, exactly as the harness named them,
# so the hook never changes directory and never second-guesses the patch.
docs=""
while IFS= read -r path; do
  case "$path" in
    *.html) ;;
    *) continue ;;
  esac
  [ -f "$path" ] || continue
  docs="${docs:+$docs }$path"
done <<<"$paths"
[ -n "$docs" ] || exit 0

# shellcheck disable=SC2086 # the newline-separated list is the argument list
out=$(${XTASK_BIN:-cargo xtask} verify-doc $docs 2>&1)
rc=$?
[ "$rc" -eq 1 ] || exit 0
printf '%s\n' "$out" >&2
exit 2
