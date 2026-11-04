import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type RefObject } from "react";

export interface MenuAction {
  label: string;
  keys?: string;
  disabled?: boolean;
  why?: string;
  checked?: boolean;
  run: () => void;
}

export interface MenuChoice {
  label: string;
  choices: { id: string; label: string }[];
  value: string | null;
  pick: (id: string) => void;
  disabled?: boolean;
  why?: string;
}

export type MenuItem = MenuAction | MenuChoice;

export interface MenuPoint {
  x: number;
  y: number;
}

const isChoice = (item: MenuItem): item is MenuChoice => "choices" in item;
const EDGE = 8;

function placement(menu: DOMRect, anchor: DOMRect | null, at: MenuPoint | undefined): MenuPoint {
  const width = window.innerWidth;
  const height = window.innerHeight;
  if (anchor) {
    const below = anchor.bottom + 4;
    const top = below + menu.height > height && anchor.top - menu.height - 4 >= 0 ? anchor.top - menu.height - 4 : below;
    return { x: Math.max(EDGE, anchor.right - menu.width), y: top };
  }
  const point = at ?? { x: 0, y: 0 };
  return {
    x: Math.max(EDGE, Math.min(point.x, width - menu.width - EDGE)),
    y: Math.max(EDGE, Math.min(point.y, height - menu.height - EDGE)),
  };
}

/**
 * Menu — a popover list of acts, opened from a trigger or at a point; each row runs an act, toggles a setting or picks one of a few choices, and shows its key.
 * @tier T2 — owns the popover, the row forms, roving focus, dismissal and placement; which acts exist, when each is disabled and what each does are the caller's
 * @anatomy Select's popup surface (canvas, hairline border, radius, shadow-pop, z-popup), 256px wide · sections split by a hairline rule, untitled · rows min-height 24, Inter text-control ink-body: 9px tick column (✓ on a check row) · label · key in ink-mute on the right · choice row: label then 2–5 inline choices, the chosen one wash-active + ink · active row wash-tint + ink · disabled row ink-mute with its reason in the key column
 * @use a screen's overflow of acts behind one trigger (the data window's ⋯) · a right-click on a selection or a row, holding only acts on what was clicked
 * @avoid picking a value that stays shown (Select) · two to four always-visible choices (SegmentedControl) · submenus — a choice row covers a few choices, a longer list is a Select · section titles · a row without a key path: every row needs a key of its own or the menu's own ↑ / ↓ · an act that is the screen's primary one (it belongs on screen) · disabling a row without a reason
 * @states open from a trigger · open at a point · a row active · a check row on · a choice row with one chosen · a disabled row with its reason. No closed, empty or loading state
 * @madeof — (Select's popup tokens, no member)
 */
export function Menu({
  label,
  sections,
  anchor,
  at,
  onClose,
}: {
  label: string;
  sections: MenuItem[][];
  anchor?: RefObject<HTMLElement | null>;
  at?: MenuPoint;
  onClose: () => void;
}) {
  const live = sections.flat().filter((item) => !item.disabled);
  const [active, setActive] = useState(0);
  const [place, setPlace] = useState<MenuPoint | null>(null);
  const surface = useRef<HTMLDivElement>(null);
  const current = live[Math.min(active, live.length - 1)];

  const close = () => {
    onClose();
    const trigger = anchor?.current;
    (trigger?.matches("button") ? trigger : trigger?.querySelector("button"))?.focus();
  };
  const closeRef = useRef(close);
  closeRef.current = close;

  useLayoutEffect(() => {
    if (!surface.current) return;
    setPlace(placement(surface.current.getBoundingClientRect(), anchor?.current?.getBoundingClientRect() ?? null, at));
  }, [anchor, at]);

  useEffect(() => {
    surface.current?.focus({ preventScroll: true });
    const outside = (event: MouseEvent) => {
      const target = event.target as Node;
      if (surface.current?.contains(target) || anchor?.current?.contains(target)) return;
      closeRef.current();
    };
    const blurred = () => closeRef.current();
    document.addEventListener("mousedown", outside);
    window.addEventListener("blur", blurred);
    return () => {
      document.removeEventListener("mousedown", outside);
      window.removeEventListener("blur", blurred);
    };
  }, [anchor]);

  const run = (item: MenuAction) => {
    item.run();
    close();
  };

  const step = (choice: MenuChoice, by: number) => {
    const count = choice.choices.length;
    const at = choice.choices.findIndex((c) => c.id === choice.value);
    const next = at < 0 ? (by > 0 ? 0 : count - 1) : (at + by + count) % count;
    choice.pick(choice.choices[next].id);
  };

  const typeAhead = (letter: string) => {
    const order = live.map((_, i) => (active + 1 + i) % live.length);
    const found = order.find((i) => live[i].label.toLowerCase().startsWith(letter.toLowerCase()));
    if (found !== undefined) setActive(found);
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const move = (to: number) => {
      event.preventDefault();
      setActive(to);
    };
    if (event.key === "ArrowDown") move((active + 1) % live.length);
    else if (event.key === "ArrowUp") move((active - 1 + live.length) % live.length);
    else if (event.key === "Home") move(0);
    else if (event.key === "End") move(live.length - 1);
    else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "Enter" && current && !isChoice(current)) {
      event.preventDefault();
      run(current);
    } else if ((event.key === "ArrowRight" || event.key === "ArrowLeft") && current && isChoice(current)) {
      event.preventDefault();
      step(current, event.key === "ArrowRight" ? 1 : -1);
    } else if (event.key === "Tab") {
      event.preventDefault();
    } else if (event.key.length === 1 && /\S/.test(event.key) && !event.metaKey && !event.ctrlKey) {
      typeAhead(event.key);
    }
  };

  const tone = (item: MenuItem) =>
    item.disabled ? "text-ink-mute" : item === current ? "bg-wash-tint text-ink" : "text-ink-body";
  const hover = (item: MenuItem) => () => {
    if (!item.disabled) setActive(live.indexOf(item));
  };

  const row = (item: MenuItem) => {
    if (isChoice(item)) {
      return (
        <div
          key={item.label}
          role="group"
          aria-label={item.label}
          aria-disabled={item.disabled || undefined}
          onMouseEnter={hover(item)}
          className={`flex min-h-control items-center gap-chrome-gap px-control-x py-control-y ${tone(item)}`}
        >
          <span aria-hidden="true" className="w-glyph shrink-0" />
          <span className="flex-1">{item.label}</span>
          {item.disabled && item.why ? (
            <span className="text-ink-mute">{item.why}</span>
          ) : (
            <span className="flex gap-chrome-tight">
              {item.choices.map((choice) => {
                const chosen = choice.id === item.value;
                return (
                  <button
                    key={choice.id}
                    type="button"
                    role="menuitemradio"
                    aria-checked={chosen}
                    disabled={item.disabled}
                    tabIndex={-1}
                    onClick={() => item.pick(choice.id)}
                    className={`rounded-button px-chrome-x ${
                      item.disabled ? "" : chosen ? "bg-wash-active text-ink" : "text-ink-body hover:bg-wash-hover"
                    }`}
                  >
                    {choice.label}
                  </button>
                );
              })}
            </span>
          )}
        </div>
      );
    }
    return (
      <div
        key={item.label}
        role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
        aria-checked={item.checked}
        aria-disabled={item.disabled || undefined}
        onMouseEnter={hover(item)}
        onClick={() => {
          if (!item.disabled) run(item);
        }}
        className={`flex min-h-control items-center gap-chrome-gap px-control-x py-control-y ${
          item.disabled ? "" : "cursor-pointer"
        } ${tone(item)}`}
      >
        <span aria-hidden="true" className="w-glyph shrink-0 text-ink">
          {item.checked ? "✓" : ""}
        </span>
        <span className="flex-1">{item.label}</span>
        <span className="text-ink-mute">{item.disabled && item.why ? item.why : (item.keys ?? "")}</span>
      </div>
    );
  };

  return (
    <div
      ref={surface}
      role="menu"
      aria-label={label}
      tabIndex={-1}
      onKeyDown={onKeyDown}
      onContextMenu={(event) => event.preventDefault()}
      style={{ left: place?.x ?? 0, top: place?.y ?? 0, visibility: place ? "visible" : "hidden" }}
      className="fixed z-popup w-pane-label rounded-card border border-hairline bg-canvas py-chrome-tight font-ui text-control shadow-pop outline-none"
    >
      {sections.map((section, n) => (
        <div key={n} className={n ? "mt-chrome-tight border-t border-hairline pt-chrome-tight" : ""}>
          {section.map(row)}
        </div>
      ))}
    </div>
  );
}
