import { createContext, useContext, type ReactNode } from "react";

export const NotesShown = createContext(false);

/**
 * Help — a note under the line it explains, drawn only while the screen's one notes switch is on.
 * @tier T1 — reads the NotesShown context and nothing else; never holds a flag of its own
 * @anatomy the line it wraps, unchanged · when notes are shown, the text under the line in Inter 11px ink-mute, pt-chrome-tight, absent from the page until then
 * @use a why line, a note, a caption or a warning that explains something on screen and is not itself something to do
 * @avoid an action or a value the user needs to act (keep it on screen) · the state of a task (StatusLine) · an outcome the user just caused (Notice) · a whole block of steps (Disclosure)
 * @states hidden · shown, both set by the NotesShown provider the screen wraps it in (DEC-66)
 * @madeof nothing beyond the line and its note
 */
export function Help({ text, children }: { text: ReactNode; children: ReactNode }) {
  const shown = useContext(NotesShown);
  return (
    <span data-testid="help">
      {children}
      {shown && (
        <span data-help="" className="block pt-chrome-tight font-ui text-chrome text-ink-mute">
          {text}
        </span>
      )}
    </span>
  );
}
