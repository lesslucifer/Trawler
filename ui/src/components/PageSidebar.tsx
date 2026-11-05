import { useRef } from "react";

export interface SidebarPage {
  value: string;
  label: string;
}

/**
 * PageSidebar — the pages of a window that has more than one, named in a column; one shown at a time.
 * @tier T1 — selection display + click/keyboard handling only; which pages exist and which shows is the screen's
 * @anatomy a column of page names at body size, a hairline on its right · the current page in ink, the rest ink-mute
 *   · a vertical tablist, roving tabindex
 * @use a window with two or more pages of its own: the Settings window's General · Capture service (E-BGSVC.W5)
 * @avoid a view or mode switch inside one page (that is SegmentedControl) · a choice of value (Select) · actions
 *   (Button) · nesting: a page holds no sidebar of its own · wrapping past the first or last page
 * @states current (ink) · other (ink-mute, ink on hover) · focus (the shared 1px mute outline). ↑/↓ moves to a page and
 *   shows it, no wrap. No disabled, empty or loading state
 * @madeof —
 */
export function PageSidebar({
  pages,
  value,
  onChange,
  ariaLabel,
}: {
  pages: ReadonlyArray<SidebarPage>;
  value: string;
  onChange: (value: string) => void;
  ariaLabel: string;
}) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);

  const step = (from: number, delta: number) => {
    const next = pages[from + delta];
    if (!next) return;
    refs.current[from + delta]?.focus();
    onChange(next.value);
  };

  return (
    <div
      role="tablist"
      aria-orientation="vertical"
      aria-label={ariaLabel}
      className="flex w-field shrink-0 flex-col items-start gap-2 border-r border-hairline pr-3"
    >
      {pages.map((page, i) => {
        const current = page.value === value;
        return (
          <button
            key={page.value}
            ref={(el) => {
              refs.current[i] = el;
            }}
            type="button"
            role="tab"
            aria-selected={current}
            tabIndex={current ? 0 : -1}
            onClick={() => {
              if (!current) onChange(page.value);
            }}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                step(i, 1);
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                step(i, -1);
              }
            }}
            className={`text-left font-ui text-body transition-colors focus-visible:outline focus-visible:outline-focus focus-visible:outline-offset-focus focus-visible:outline-ink-mute ${
              current ? "text-ink" : "text-ink-mute hover:text-ink"
            }`}
          >
            {page.label}
          </button>
        );
      })}
    </div>
  );
}
