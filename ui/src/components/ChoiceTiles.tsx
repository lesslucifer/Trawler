import { useRef } from "react";

export interface TileOption {
  id: string;
  label: string;
}

/**
 * ChoiceTiles — one choice over four to eight options, every option visible at once.
 * @tier T1 — selection display and key handling only; which options exist, what they mean and what follows from the pick are the tenant's
 * @anatomy a wrapping row of tiles on the 8px gap · each tile a 1px hairline border at radius 3, padding 3/9, Inter 11.5px, ink-mute at rest and ink with an ink-mute border when taken · the row wraps rather than scrolls, so a long label lengthens the column and never a scroller · radiogroup semantics, roving tabindex, no popup and no trigger of its own
 * @use the one question a screen is organised around, where the reader must SEE the alternatives to answer it and there are too many for a segment group — the setup screen's eight platforms
 * @avoid two to three options (that is SegmentedControl, and it reads tighter) · more than eight, option groups, search or any choice worth hiding behind a trigger (that is Select) · multi-select and toggles (that is Checkbox) · an action per tile (that is Button) · a tile carrying state of its own — a tile is an option, and what the choice made of it belongs below the group
 * @states one tile taken (exactly one, always) · hover and focus per tile · disabled (whole group). No empty state — a group with nothing to choose between is not rendered
 * @madeof —
 */
export function ChoiceTiles({
  options,
  value,
  onChange,
  ariaLabel,
  disabled = false,
  className = "",
}: {
  options: ReadonlyArray<TileOption>;
  value: string;
  onChange: (id: string) => void;
  ariaLabel: string;
  disabled?: boolean;
  className?: string;
}) {
  const tiles = useRef<(HTMLButtonElement | null)[]>([]);

  // Focus moves without committing, so a reader can look through the options
  // with the keyboard exactly as they look through them with their eyes. No
  // wrap: the ends of the row are where the row ends.
  const moveFocus = (from: number, delta: number) => {
    const next = from + delta;
    if (next < 0 || next >= options.length) return;
    tiles.current[next]?.focus();
  };

  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      className={`flex flex-wrap gap-2 ${disabled ? "opacity-disabled" : ""} ${className}`}
    >
      {options.map((option, i) => {
        const taken = option.id === value;
        return (
          <button
            key={option.id}
            ref={(el) => {
              tiles.current[i] = el;
            }}
            type="button"
            role="radio"
            aria-checked={taken}
            disabled={disabled}
            tabIndex={taken ? 0 : -1}
            onClick={() => {
              if (!taken) onChange(option.id);
            }}
            onKeyDown={(e) => {
              if (e.key === "ArrowRight" || e.key === "ArrowDown") {
                e.preventDefault();
                moveFocus(i, 1);
              } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
                e.preventDefault();
                moveFocus(i, -1);
              }
            }}
            className={`cursor-pointer rounded-button border px-chrome-x py-control-y font-ui text-control transition-colors focus-visible:outline focus-visible:outline-focus focus-visible:outline-offset-focus focus-visible:outline-ink-mute disabled:pointer-events-none disabled:opacity-disabled ${
              taken
                ? "border-ink-mute text-ink"
                : "border-hairline text-ink-mute hover:bg-wash-hover hover:text-ink"
            }`}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
