import { useEffect, useId, useImperativeHandle, useRef, useState, type KeyboardEvent, type Ref } from "react";
import { chipLabel, hasOpenQuote, parseDraft, sameChip, suggest, type Chip, type Suggestion } from "../query";
import { IconButton } from "./IconButton";
import { MethodLabel } from "./MethodLabel";
import { Notice } from "./Notice";

export interface QueryBarProps {
  chips: readonly Chip[];
  onChange: (chips: Chip[]) => void;
  hosts: readonly string[];
  refused?: (chip: Chip) => boolean;
  className?: string;
  handle?: Ref<QueryBarHandle>;
}

export interface QueryBarHandle {
  focus: () => void;
}

const never = () => false;

function ChipView({ chip, refused, onRemove }: { chip: Chip; refused: boolean; onRemove: () => void }) {
  const label = chipLabel(chip);
  const quoted = chip.key === "q" && label.endsWith('"');
  return (
    <span
      data-testid="chip"
      aria-invalid={refused || undefined}
      className={`inline-flex min-h-control items-center rounded-button border bg-wash-tint pl-chrome-x font-mono text-control text-ink ${refused ? "border-warn" : "border-transparent"}`}
    >
      {refused && (
        <span aria-hidden="true" className="mr-chrome-tight text-warn">
          ⚠
        </span>
      )}
      {chip.negated && <span className="text-ink-mute">-</span>}
      {chip.key !== "q" && <span className="text-ink-mute">{chip.key}:</span>}
      {chip.key === "method" ? (
        <MethodLabel method={chip.value} />
      ) : (
        <span className="break-all" data-wire={chip.key === "host" ? "host" : undefined}>
          {quoted ? `"${chip.value}"` : chip.value}
        </span>
      )}
      <IconButton glyph="close" size={24} label={`remove ${label}`} onClick={onRemove} />
    </span>
  );
}

function SuggestionText({ row }: { row: Suggestion }) {
  if (row.key !== "method" || row.kind === "key" || row.value === undefined) return <>{row.text}</>;
  return (
    <>
      <span className="text-ink-mute">{row.text.startsWith("-") ? "-" : ""}method:</span>
      <MethodLabel method={row.value} />
    </>
  );
}

/**
 * QueryBar — one box of filter chips and a draft; suggestions follow the draft, and a chip is made only when the draft parses.
 * @tier T2 — owns the grammar (draft → chip or fault, draft → suggestions) through query.ts; chips, hosts seen and the refusal are the caller's
 * @anatomy Field's box, flex-wrap, chrome-tight padding · chips: wash-tint pill, mono, key and "-" ink-mute, value ink (MethodLabel for a method), IconButton × size 24 · refused: warn border + ⚠ · draft: borderless mono input, min-w-field-sm · suggestions: Select's popup surface under the draft, completion mono + hint ui + the key that takes it · fault: an inline warn Notice in the suggestions' place
 * @use the one filter of a list the user narrows by typed keys and text: the flow list's header. Enter or Tab takes the highlighted suggestion (a key goes into the draft, a value becomes a chip), Space outside quotes takes the draft as typed, ↑/↓ move the highlight without wrapping, Esc closes the suggestions, Backspace in an empty draft removes the last chip, and each chip's × removes that one. A text chip with a space is typed in quotes. An unknown key is a fault only within two edits of a real one; any other word:rest is text. The screen binds ⌘F to focus() on the handle it passes
 * @avoid one free-text value (Field) · one choice among options (Select) · actions or commands in the bar (DEC-71 rule 9) · calling the caller on a keystroke: onChange is for chips only · editing a chip in place
 * @states rest · focused (suggestions, Enter or Tab takes the highlight, Space the draft as typed) · moved (↑/↓ highlight) · faulted (warn Notice until the next keystroke) · chips · a refused chip. No disabled, loading or restored state
 * @madeof IconButton · MethodLabel · Notice
 */
export function QueryBar({ chips, onChange, hosts, refused = never, className = "", handle }: QueryBarProps) {
  const [draft, setDraft] = useState("");
  const [open, setOpen] = useState(false);
  const [moved, setMoved] = useState<number | null>(null);
  const [fault, setFault] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const rowRefs = useRef<(HTMLLIElement | null)[]>([]);
  const listId = useId();

  useImperativeHandle(handle, () => ({ focus: () => input.current?.focus() }), []);

  const rows = open && fault === null ? suggest(draft, hosts) : [];
  const active = moved === null ? Math.max(0, rows.findIndex((row) => row.kind !== "typed")) : Math.min(moved, rows.length - 1);

  useEffect(() => {
    rowRefs.current[active]?.scrollIntoView?.({ block: "nearest" });
  }, [active]);

  const restart = (text: string) => {
    setDraft(text);
    setFault(null);
    setMoved(null);
  };

  const accept = (text: string) => {
    const parsed = parseDraft(text);
    if ("fault" in parsed) {
      setFault(parsed.fault);
      setOpen(true);
      return;
    }
    if (!("chip" in parsed)) return;
    restart("");
    const made = parsed.chip;
    if (!chips.some((chip) => sameChip(chip, made))) onChange([...chips, made]);
  };

  const take = (row: Suggestion) => (row.kind === "key" ? restart(row.text) : accept(row.text));

  const remove = (gone: Chip) => {
    onChange(chips.filter((chip) => chip !== gone));
    input.current?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const highlighted = rows[active];
    switch (event.key) {
      case "ArrowDown":
      case "ArrowUp":
        if (rows.length === 0) return;
        event.preventDefault();
        setMoved(Math.min(Math.max(active + (event.key === "ArrowDown" ? 1 : -1), 0), rows.length - 1));
        return;
      case "Tab":
        if (event.shiftKey) return;
        if (highlighted) {
          event.preventDefault();
          take(highlighted);
        } else if (draft.trim()) {
          event.preventDefault();
          accept(draft);
        }
        return;
      case "Enter":
        event.preventDefault();
        if (highlighted) take(highlighted);
        else if (draft.trim()) accept(draft);
        return;
      case " ":
        if (!draft.trim() || hasOpenQuote(draft)) return;
        event.preventDefault();
        accept(draft);
        return;
      case "Escape":
        setOpen(false);
        setMoved(null);
        return;
      case "Backspace":
        if (draft !== "" || chips.length === 0) return;
        event.preventDefault();
        onChange(chips.slice(0, -1));
        return;
    }
  };

  const expanded = open && (fault !== null || rows.length > 0);

  return (
    <div
      onMouseDown={(event) => {
        if (event.target !== event.currentTarget) return;
        event.preventDefault();
        input.current?.focus();
      }}
      className={`flex flex-wrap items-center gap-chrome-tight rounded-button border border-hairline bg-canvas p-chrome-tight transition-colors hover:border-ink-mute focus-within:border-ink-mute ${className}`}
    >
      {chips.map((chip) => (
        <ChipView key={chipLabel(chip)} chip={chip} refused={refused(chip)} onRemove={() => remove(chip)} />
      ))}
      <div className="relative min-w-field-sm flex-1">
        <input
          ref={input}
          type="text"
          role="combobox"
          aria-label="filter"
          aria-autocomplete="list"
          aria-expanded={expanded}
          aria-controls={listId}
          aria-activedescendant={rows.length > 0 ? `${listId}-${active}` : undefined}
          aria-invalid={fault !== null || undefined}
          spellCheck={false}
          autoComplete="off"
          value={draft}
          placeholder={chips.length > 0 ? "add a filter" : "filter — status:4xx, host:api, or any text"}
          onChange={(event) => {
            restart(event.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onBlur={() => {
            setOpen(false);
            setMoved(null);
          }}
          onKeyDown={onKeyDown}
          className="h-control w-full border-none bg-transparent px-chrome-x font-mono text-control text-ink outline-none placeholder:font-ui placeholder:text-ink-mute"
        />
        {expanded && (
          <div className="absolute left-0 top-full z-popup mt-chrome-y min-w-picker overflow-hidden rounded-card border border-hairline bg-canvas shadow-pop">
            {fault !== null ? (
              <div className="px-control-x py-control-y">
                <Notice tone="warn">{fault}</Notice>
              </div>
            ) : (
              <ul id={listId} role="listbox" aria-label="suggestions" className="max-h-listing-max overflow-y-auto py-chrome-tight">
                {rows.map((row, i) => (
                  <li
                    key={`${row.kind}${row.text}`}
                    id={`${listId}-${i}`}
                    ref={(el) => {
                      rowRefs.current[i] = el;
                    }}
                    role="option"
                    aria-selected={i === active}
                    onMouseDown={(event) => {
                      event.preventDefault();
                      take(row);
                    }}
                    onMouseEnter={() => setMoved(i)}
                    className={`flex min-h-control cursor-pointer items-center gap-2 whitespace-nowrap px-control-x py-control-y text-control ${i === active ? "bg-wash-tint text-ink" : "text-ink-body"}`}
                  >
                    <span className="font-mono">
                      <SuggestionText row={row} />
                    </span>
                    {row.hint && <span className="font-ui text-ink-mute">{row.hint}</span>}
                    <span className="ml-auto pl-2 font-ui text-chrome text-ink-mute">{i === active ? "↵ enter" : ""}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
