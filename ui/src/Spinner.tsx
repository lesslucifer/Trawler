// The in-flight ring Select and StatusLine each hand-roll inline: a hairline
// circle with one arc inked, turning. Build-local and a module of its own for
// the same reason ModalShell is — no registry entry owns it, promoting it is the
// collection's to perform, and the alternative here was a third and a fourth
// copy of the same six classes.
//
// Decorative by construction: it says "still running", which the line beside it
// already says in words, so it carries no accessible name of its own.
export function Spinner() {
  return (
    <span
      aria-hidden="true"
      data-testid="spinner"
      className="inline-block h-spinner w-spinner shrink-0 animate-spin rounded-full border-emphasis border-hairline border-t-ink-mute"
    />
  );
}
