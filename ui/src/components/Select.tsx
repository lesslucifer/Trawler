import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";

export interface SelectOption {
  id: string;
  label: string;
  element?: ReactNode;
}

/**
 * Select — the app's one dropdown: a compact trigger, a popup list, and type-to-filter.
 * @tier T1 — presentation and keyboard behaviour only; options, the selected value, the loading flag and the handler all come from the caller. It never fetches, sorts, resolves or defaults anything
 * @anatomy trigger = CopyButton's box (1px line border, radius 3, padding 3/9, min-height 24, Inter 11.5px ink-body) plus a muted ▾ caret and min-width 132 so it never resizes with the selection; shows the selected label, or the placeholder in ink-mute when value is undefined · popup overlays below (canvas, hairline border, radius 4, shadow), never pushes content · search row when searchable (default true) or as soon as the user types: borderless input, case-insensitive substring on label, matched run underlined · option rows with a reserved 9px tick column, min-height 24, one line: the option's element if given, else its label; selected = ✓ in ink, keyboard-active = soft wash — two channels, not one · one muted-italic line replaces the list for the three non-list outcomes, in precedence order: "loading…" with a spinner (isLoading) · "nothing to choose from" (no options) · "no match" (filter) · list scrolls past ~7 rows
 * @use any single choice from five or more options, whether they are known at build time or fetched: a target picker, a filter, a saved view, an export format. Pass {id,label,element?}[] plus the value and a handler; value may be undefined for nothing-selected, and isLoading covers options still on their way
 * @avoid two to four options (use SegmentedControl — all choices visible reads as a switch; this is a miss by cardinality, not kind) · multi-select or a clear affordance — at most one option is selected, and nothing returns to unselected once chosen · free-text entry (that is a combobox, not this) · option groups or disabled entries · an option element that needs two lines or its own hit target: the row is one line at a 24px floor whatever the element draws · an element without a label — the trigger and the filter both need text · fetching, sorting or defaulting options inside the component · explaining a failed fetch here: stop passing isLoading and render your own Notice, a 184px popup cannot carry an error and a retry · committing on arrow-key movement: only Enter or a click selects
 * @states rest · hover · focus (1px mute outline) · open · filtering · no-match · loading (spinner, outranks empty) · empty (loaded, no options) · nothing-selected (value undefined → muted placeholder, no tick) · disabled (38%, out of tab order). Keyboard: ↑↓ move without selecting (no wrap), Enter selects (a no-op while loading, empty or filtered to nothing), Esc closes unchanged, Tab closes and moves on, typing filters even when searchable is false; closing always returns focus to the trigger. No invalid, partial-selection or open-upward state
 * @madeof —
 */
export function Select({
  options,
  value,
  onChange,
  placeholder = "select…",
  isLoading = false,
  searchable = true,
  label,
  disabled = false,
  className = "",
}: {
  options: SelectOption[];
  value: string | undefined;
  onChange: (id: string) => void;
  placeholder?: string;
  isLoading?: boolean;
  searchable?: boolean;
  label?: string;
  disabled?: boolean;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const [active, setActive] = useState(0);
  const trigger = useRef<HTMLButtonElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const rows = useRef<(HTMLLIElement | null)[]>([]);

  const visible = useMemo(
    () =>
      filter
        ? options.filter((o) => o.label.toLowerCase().includes(filter.toLowerCase()))
        : options,
    [options, filter],
  );

  const selected = options.find((o) => o.id === value);
  const typed = filter.length > 0;

  const close = () => {
    setOpen(false);
    setFilter("");
    trigger.current?.focus();
  };

  const openWith = () => {
    const at = visible.findIndex((o) => o.id === value);
    setActive(at < 0 ? 0 : at);
    setOpen(true);
  };

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) {
        setOpen(false);
        setFilter("");
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  useEffect(() => {
    if (open) rows.current[active]?.scrollIntoView?.({ block: "nearest" });
  }, [open, active]);

  // Options can change under an open popup (remote options arriving, a re-fetch
  // reordering them). A surviving index silently points at a different row, so
  // Enter would select an option the user never looked at.
  useEffect(() => {
    setActive(0);
  }, [options]);

  const selectable = !isLoading && visible.length > 0;

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (!open) {
      if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        openWith();
      }
      return;
    }

    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "Tab") {
      setOpen(false);
      setFilter("");
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => Math.min(i + 1, visible.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (selectable && visible[active]) {
        onChange(visible[active].id);
        close();
      }
    } else if (e.key.length === 1) {
      setFilter((f) => f + e.key);
      setActive(0);
    }
  };

  const message = isLoading
    ? "loading…"
    : options.length === 0
      ? "nothing to choose from"
      : visible.length === 0
        ? "no match"
        : null;

  return (
    <div ref={root} className={`relative inline-block ${className}`} onKeyDown={onKeyDown}>
      <button
        ref={trigger}
        type="button"
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={label}
        onClick={() => (open ? close() : openWith())}
        className="inline-flex min-h-control min-w-[132px] cursor-pointer select-none items-center justify-between gap-2 whitespace-nowrap rounded-button border border-hairline bg-transparent px-control-x py-control-y font-ui text-control text-ink-body transition-colors hover:border-ink-mute hover:text-ink focus-visible:outline focus-visible:outline-1 focus-visible:outline-ink-mute focus-visible:outline-offset-2 disabled:pointer-events-none disabled:opacity-disabled"
      >
        <span className={selected ? "" : "text-ink-mute"}>{selected?.label ?? placeholder}</span>
        <span aria-hidden="true" className="shrink-0 text-glyph leading-none text-ink-mute">
          ▾
        </span>
      </button>

      {open && (
        <div className="absolute left-0 top-[calc(100%+4px)] z-20 min-w-full overflow-hidden rounded-card border border-hairline bg-canvas shadow-pop">
          {(searchable || typed) && (
            <div className="flex items-center gap-chrome-gap border-b border-hairline px-2 py-1">
              <input
                autoFocus
                value={filter}
                aria-label="filter options"
                onChange={(e) => {
                  setFilter(e.target.value);
                  setActive(0);
                }}
                onKeyDown={(e) => {
                  if (e.key.length === 1) e.stopPropagation();
                }}
                className="min-w-0 flex-1 border-none bg-transparent font-ui text-control text-ink outline-none placeholder:text-ink-mute"
                placeholder="filter…"
              />
            </div>
          )}

          {message ? (
            <p className="px-control-x py-[5px] font-ui text-control italic text-ink-mute">
              {isLoading && (
                <span
                  aria-hidden="true"
                  className="mr-chrome-gap inline-block h-spinner w-spinner animate-spin rounded-full border-[1.5px] border-hairline border-t-ink-mute"
                />
              )}
              {message}
            </p>
          ) : (
            <ul role="listbox" aria-label={label} className="max-h-[184px] overflow-y-auto py-0.5">
              {visible.map((option, i) => (
                <li
                  key={option.id}
                  ref={(el) => {
                    rows.current[i] = el;
                  }}
                  role="option"
                  aria-label={option.label}
                  aria-selected={option.id === value}
                  onClick={() => {
                    onChange(option.id);
                    close();
                  }}
                  onMouseEnter={() => setActive(i)}
                  className={`flex min-h-control cursor-pointer items-center gap-chrome-gap whitespace-nowrap px-control-x py-control-y font-ui text-control ${
                    i === active ? "bg-wash-tint text-ink" : "text-ink-body"
                  }`}
                >
                  <span aria-hidden="true" className="w-glyph shrink-0 text-ink">
                    {option.id === value ? "✓" : ""}
                  </span>
                  {option.element ?? <Highlight text={option.label} match={filter} />}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}

function Highlight({ text, match }: { text: string; match: string }) {
  if (!match) return <>{text}</>;
  const at = text.toLowerCase().indexOf(match.toLowerCase());
  if (at < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, at)}
      <span className="text-ink underline decoration-ink-mute underline-offset-2">
        {text.slice(at, at + match.length)}
      </span>
      {text.slice(at + match.length)}
    </>
  );
}
