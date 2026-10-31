import { Fragment } from "react";

/**
 * MenuPath — the controls a step clicks through, each name set apart from the sentence that leads to it.
 * @tier T1 — props in, a span out; no state and no business logic
 * @anatomy inline, wrapping between names and never inside one · each name in ink, a tone above the sentence's ink-body, since the UI renders one weight · → in ink-mute between names · size and family inherited from the sentence
 * @use a menu, settings or dialog path inside a step's sentence — Keychain Access, System Settings, a browser's certificate manager — and one control's name the user must find
 * @avoid a file path or a value (that is CopyText — it is typed, not looked for) · a command (CommandBlock) · mono: a menu name is not wire data · anything clickable: these name another app's controls · a path outside a step's sentence
 * @states one — it is text
 * @madeof —
 */
export function MenuPath({ path }: { path: readonly string[] }) {
  return (
    <span data-testid="menu-path" className="inline-flex flex-wrap items-baseline gap-x-chrome-gap">
      {path.map((name, i) => (
        <Fragment key={`${i}:${name}`}>
          {i > 0 && <span className="text-ink-mute">→</span>}
          <span className="whitespace-nowrap text-ink">{name}</span>
        </Fragment>
      ))}
    </span>
  );
}
