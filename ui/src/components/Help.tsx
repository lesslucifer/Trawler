import { useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { IconButton } from "./IconButton";

/**
 * Help — a bare ? at the end of a line that opens what explains that line underneath it.
 * @tier T1 — one open flag of its own; never reads what it shows
 * @anatomy the line it wraps, unchanged · IconButton glyph="help" after it, ml-chrome-gap, its ? on the line's baseline, its 24px box pulled in by -my-chrome-y so it never makes the line taller · when open, the text under the line in Inter 11px ink-mute, pt-chrome-tight, absent from the page until then
 * @use a why line, a note, a caption or a warning that explains something on screen and is not itself something to do
 * @avoid an action or a value the user needs to act (keep it on screen) · the state of a task (StatusLine) · an outcome the user just caused (Notice) · a whole block of steps (Disclosure)
 * @states closed · open (the ? in ink) · hover and focus from IconButton. Esc closes it with focus inside and returns focus to the ?
 * @madeof IconButton (T1) with glyph="help", expanded and controls
 */
export function Help({ topic, text, children }: { topic: string; text: ReactNode; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const panel = useId();
  const trigger = useRef<HTMLSpanElement>(null);

  const closeOnEscape = (event: KeyboardEvent) => {
    if (event.key !== "Escape" || !open) return;
    event.stopPropagation();
    setOpen(false);
    trigger.current?.querySelector("button")?.focus();
  };

  return (
    <span data-testid="help" onKeyDown={closeOnEscape}>
      {children}
      <span ref={trigger} className={`-my-chrome-y ml-chrome-gap inline-flex align-baseline ${open ? "[&>button]:text-ink" : ""}`}>
        <IconButton
          glyph="help"
          label={`help: ${topic}`}
          size={24}
          expanded={open}
          controls={panel}
          onClick={() => setOpen(!open)}
        />
      </span>
      {open && (
        <span id={panel} data-help="" className="block pt-chrome-tight font-ui text-chrome text-ink-mute">
          {text}
        </span>
      )}
    </span>
  );
}
