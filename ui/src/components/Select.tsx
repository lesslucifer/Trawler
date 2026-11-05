import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";

export interface SelectOption {
  id: string;
  label: string;
  element?: ReactNode;
}

/**
 * Select — the app's one dropdown: a compact trigger, a popup list, and type-to-filter.
 * @tier T1 — presentation and keyboard behaviour only; options, the selected value, the loading flag and the handler all come from the caller. It never fetches, sorts, resolves or defaults anything
 * @anatomy trigger = CopyButton's box (1px line border, radius 3, padding 3/9, min-height 24, Inter 11.5px ink-body) plus a muted ▾ caret; it fills the component's own box and carries a 132 minimum, so it never resizes with the selection and a host that needs a wider one sizes the box through className; shows the selected label, or the placeholder in ink-mute when value is undefined · popup overlays below, or above with opens="above" for a trigger on a window's bottom edge (canvas, hairline border, radius 4, shadow), never pushes content · search row when searchable (default true) or as soon as the user types: borderless input, case-insensitive substring on label, matched run underlined · option rows with a reserved 9px tick column, min-height 24, one line: the option's element if given, else its label; selected = ✓ in ink, keyboard-active = soft wash — two channels, not one · one muted-italic line replaces the list for the three non-list outcomes, in precedence order: "loading…" with a spinner (isLoading) · "nothing to choose from" (no options) · "no match" (filter) · list scrolls past ~7 rows
 * @use any single choice from five or more options, whether they are known at build time or fetched: a target picker, a saved view, an export format. Pass {id,label,element?}[] plus the value and a handler; value may be undefined for nothing-selected, and isLoading covers options still on their way
 * @avoid two to four options (use SegmentedControl — all choices visible reads as a switch; this is a miss by cardinality, not kind) · multi-select or a clear affordance — at most one option is selected, and nothing returns to unselected once chosen, so a filter that must clear offers an "any" option of its own rather than asking for a clear button · free-text entry (that is Field, or QueryBar for a list's typed filter) · option groups or disabled entries · an option element that needs two lines or its own hit target: the row is one line at a 24px floor whatever the element draws · an element without a label — the trigger and the filter both need text · fetching, sorting or defaulting options inside the component · explaining a failed fetch here: stop passing isLoading and render your own Notice, a 184px popup cannot carry an error and a retry · committing on arrow-key movement: only Enter or a click selects
 * @states rest · hover · focus (1px mute outline) · open · filtering · no-match · loading (spinner, outranks empty) · empty (loaded, no options) · nothing-selected (value undefined → muted placeholder, no tick) · disabled (38%, out of tab order). Keyboard: ↑↓ move without selecting (no wrap), Enter selects (a no-op while loading, empty or filtered to nothing), Esc closes unchanged, Tab closes and moves on, typing filters even when searchable is false; closing always returns focus to the trigger. No invalid or partial-selection state
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
  opens = "below",
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
  opens?: "below" | "above";
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
  // reordering them). An index that survives that points at a different row, so
  // Enter would select an option the user never looked at — while a parent that
  // rebuilds an unchanged array every render must NOT move the highlight, which
  // is the commoner case. Reconciling by id satisfies both. Keyed on OPTIONS, not
  // on the filtered list: narrowing by filter deliberately resets to the first
  // match, and reconciling there would override that.
  const optionIds = options.map((o) => o.id).join("\u0000");
  const knownIds = useRef(optionIds);
  const activeId = useRef(visible[active]?.id);
  if (knownIds.current === optionIds) {
    activeId.current = visible[active]?.id;
  } else {
    knownIds.current = optionIds;
    const moved = visible.findIndex((o) => o.id === activeId.current);
    if (moved !== active) setActive(moved < 0 ? 0 : moved);
  }

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
        className="inline-flex w-full min-h-control min-w-control-min cursor-pointer select-none items-center justify-between gap-2 whitespace-nowrap rounded-button border border-hairline bg-transparent px-control-x py-control-y font-ui text-control text-ink-body transition-colors hover:border-ink-mute hover:text-ink focus-visible:outline focus-visible:outline-focus focus-visible:outline-ink-mute focus-visible:outline-offset-focus disabled:pointer-events-none disabled:opacity-disabled"
      >
        <span className={selected ? "" : "text-ink-mute"}>{selected?.label ?? placeholder}</span>
        <span aria-hidden="true" className="shrink-0 text-glyph leading-none text-ink-mute">
          ▾
        </span>
      </button>

      {open && (
        <div
          data-opens={opens}
          className={`absolute left-0 ${
            opens === "above"
              ? // design-allow: bottom-[calc(100%+theme(spacing.chrome-y))] — an offset-from-the-trigger shape; the gap itself is a token
                "bottom-[calc(100%+theme(spacing.chrome-y))]"
              : // design-allow: top-[calc(100%+theme(spacing.chrome-y))] — an offset-from-the-trigger shape; the gap itself is a token
                "top-[calc(100%+theme(spacing.chrome-y))]"
          } z-popup min-w-full overflow-hidden rounded-card border border-hairline bg-canvas shadow-pop`}
        >
          {(searchable || typed) && (
            <div className="flex items-center gap-chrome-gap border-b border-hairline px-2 py-chrome-y">
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
            <p className="px-control-x py-control-y font-ui text-control italic text-ink-mute">
              {isLoading && (
                <span
                  aria-hidden="true"
                  className="mr-chrome-gap inline-block h-spinner w-spinner animate-spin rounded-full border-emphasis border-hairline border-t-ink-mute"
                />
              )}
              {message}
            </p>
          ) : (
            <ul role="listbox" aria-label={label} className="max-h-listing-max overflow-y-auto py-chrome-tight">
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
      <span className="text-ink underline decoration-ink-mute underline-offset-link">
        {text.slice(at, at + match.length)}
      </span>
      {text.slice(at + match.length)}
    </>
  );
}
