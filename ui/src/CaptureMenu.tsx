import { useEffect, useRef, useState } from "react";
import { IconButton } from "./components/index";
import { Spinner } from "./Spinner";

export interface CaptureAct {
  id: string;
  label: string;
  /** The accessible name, where the visible label leans on the menu for context. */
  name?: string;
  /**
   * The act's face — the character the rest of the app already spends on this
   * direction, never a new one. Decorative to a reader: the label is the name.
   */
  glyph?: string;
  /** Turns the ring in the glyph's box: this act is the one currently running. */
  busy?: boolean;
  disabled?: boolean;
  /** Draws the hairline above this item — the additive/irreversible boundary. */
  separated?: boolean;
  run: () => void;
}

// M — an action menu, built local. Nothing in the collection is one: Select is
// the only popup and excludes itself twice in its own @avoid (a cardinality
// floor of four, and a held selection a menu of acts never has). Promoting a
// Menu — or extracting the popup shell Select already owns — is design run 11's
// proposal and the collection's to perform, so this ships the behaviour without
// minting an entry.
//
// Everything below is the rulebook's own keyboard convention, not a new one:
// ↑↓ move without acting, Enter acts, Esc closes unchanged, Tab closes and moves
// on, and every exit returns focus to the trigger.
export function CaptureMenu({ acts, label }: { acts: CaptureAct[]; label: string }) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const trigger = useRef<HTMLDivElement>(null);
  const root = useRef<HTMLDivElement>(null);

  const close = (returnFocus: boolean) => {
    setOpen(false);
    if (returnFocus) triggerButton()?.focus();
  };

  const triggerButton = () => trigger.current?.querySelector<HTMLElement>("button");

  const openMenu = () => {
    setActive(0);
    setOpen(true);
  };

  const act = (item: CaptureAct) => {
    if (item.disabled) return;
    close(true);
    item.run();
  };

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (!open) {
      if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        openMenu();
      }
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      close(true);
    } else if (e.key === "Tab") {
      close(false);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => Math.min(i + 1, acts.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      const item = acts[active];
      if (item) act(item);
    }
  };

  return (
    <div ref={root} className="relative inline-block" onKeyDown={onKeyDown}>
      <div ref={trigger}>
        <IconButton label={label} onClick={() => (open ? close(true) : openMenu())}>
          ⋮
        </IconButton>
      </div>
      {open && (
        <div
          className={
            // design-allow: top-[calc(100%+theme(spacing.chrome-y))] — Select's own offset-from-the-trigger shape; the gap itself is a token
            "absolute right-0 top-[calc(100%+theme(spacing.chrome-y))] z-popup overflow-hidden rounded-card border border-hairline bg-canvas shadow-pop"
          }
        >
          <ul role="menu" aria-label={label} className="py-chrome-tight">
            {acts.map((item, i) => (
              <li
                key={item.id}
                role="menuitem"
                aria-label={item.name ?? item.label}
                aria-disabled={item.disabled}
                tabIndex={-1}
                onClick={() => act(item)}
                onMouseEnter={() => setActive(i)}
                className={`flex min-h-control min-w-control-min cursor-pointer items-center whitespace-nowrap px-control-x py-control-y font-ui text-control ${
                  item.separated ? "mt-chrome-tight border-t border-hairline pt-control-y" : ""
                } ${item.disabled ? "cursor-default text-ink-mute opacity-disabled" : ""} ${
                  i === active && !item.disabled ? "bg-wash-tint text-ink" : "text-ink-body"
                }`}
              >
                <span
                  aria-hidden="true"
                  className="mr-chrome-gap inline-flex w-4 shrink-0 items-center justify-center text-ink-mute"
                >
                  {item.busy ? <Spinner /> : item.glyph}
                </span>
                {item.label}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
