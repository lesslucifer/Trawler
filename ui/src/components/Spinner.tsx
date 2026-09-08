/**
 * Spinner — a ring that turns while an act the user started is still running.
 * @tier T1 — one shape, no props, no state: the only thing in the app that moves
 * @anatomy a spinner-sized square span, fully rounded, hairline border on all four edges with the top edge inked to ink-mute so one arc reads darker than the other three · turns once per second, linear, forever · shrink-0, so a flex row cannot squeeze it out of round · no text, no colour of its own, no size but the one
 * @use beside a word naming what is awaited, for an act the user started whose duration the app cannot predict — a transfer, a load, a request in flight. Always the second channel behind the words: the header slot's "exporting…"/"importing…", the import ask's own line, the menu row of the act that is running
 * @avoid being the only signal that something is happening (the words carry it, this repeats it) · work with a knowable extent, where a count or a proportion tells the user more · a page-level or blocking overlay · a wait whose REASON is worth a sentence (that is Notice) · giving it an accessible name, which would make a screen reader say its neighbour's line twice
 * @states one — it turns. No rest, hover, focus or disabled state; a caller that wants it gone unmounts it
 * @madeof — nothing; a bordered span and the config's spin keyframe
 */
export function Spinner() {
  return (
    <span
      aria-hidden="true"
      data-testid="spinner"
      className="inline-block h-spinner w-spinner shrink-0 animate-spin rounded-full border-emphasis border-hairline border-t-ink-mute"
    />
  );
}
