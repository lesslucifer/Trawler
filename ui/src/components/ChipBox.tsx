import { useEffect, useId, useImperativeHandle, useRef, useState, type KeyboardEvent, type ReactNode, type Ref } from "react";
import { IconButton } from "./IconButton";
import { Notice } from "./Notice";

export interface ChipFace {
  text: string;
  key?: string;
  negated?: boolean;
  value: ReactNode;
  wire?: string;
}

export interface BoxSuggestion {
  kind: "typed" | "key" | "value";
  text: string;
  hint: string;
  face?: ReactNode;
}

export type BoxParsed<C> = { chip: C } | { fault: string } | { empty: true };

export interface ChipGrammar<C> {
  parse: (draft: string) => BoxParsed<C>;
  suggest: (draft: string) => BoxSuggestion[];
  label: (chip: C) => ChipFace;
  hasOpenQuote: (draft: string) => boolean;
}

export interface ChipBoxProps<C> {
  chips: readonly C[];
  onChange: (chips: C[]) => void;
  grammar: ChipGrammar<C>;
  refused?: (chip: C) => string | null;
  placeholder: string;
  initialDraft?: string;
  chippedPlaceholder?: string;
  label: string;
  onDraft?: (draft: string) => void;
  onEnter?: (shift: boolean) => void;
  onEscape?: () => void;
  className?: string;
  handle?: Ref<ChipBoxHandle>;
}

export interface ChipBoxHandle {
  focus: () => void;
}

const never = () => null;

function ChipView({ face, reason, refused, onRemove }: { face: ChipFace; reason: string | null; refused: boolean; onRemove: () => void }) {
  return (
    <span
      data-testid="chip"
      aria-invalid={refused || undefined}
      title={reason || undefined}
      className={`inline-flex min-h-control items-center rounded-button border bg-wash-tint pl-chrome-x font-mono text-control text-ink ${refused ? "border-warn" : "border-transparent"}`}
    >
      {refused && (
        <span aria-hidden="true" className="mr-chrome-tight text-warn">
          ⚠
        </span>
      )}
      {face.negated && <span className="text-ink-mute">-</span>}
      {face.key && <span className="text-ink-mute">{face.key}:</span>}
      <span className="break-all" data-wire={face.wire}>
        {face.value}
      </span>
      <IconButton glyph="close" size={24} label={`remove ${face.text}`} onClick={onRemove} />
    </span>
  );
}

/**
 * ChipBox — one box of typed chips and a draft: a draft becomes a chip when the caller's grammar parses it, suggestions follow it, a fault says why it did not, and a chip the caller refuses is marked.
 * @tier T1 — owns the box, chips, the draft's keys, the suggestion list, the fault slot and the refused face; what parses, what is suggested and what is refused are the caller's grammar
 * @anatomy Field's box, flex-wrap · chips: wash-tint pill, mono, key ink-mute, value ink, IconButton × 24 · refused: warn border + ⚠, reason as title · draft: borderless mono input, min-w-field-sm · suggestions: Select's popup under the draft · fault: inline warn Notice in their place
 * @use a typed search over keys and text: the flow list's filter (QueryBar) and the data window's find. Enter or Tab takes the highlighted suggestion (a key goes into the draft, a value becomes a chip), Space outside quotes takes the draft as typed, ↑/↓ move the highlight without wrapping, Backspace in an empty draft removes the last chip. With onEnter, Enter calls it instead of taking the draft, unless the user moved to a row or the highlighted row is a value (a key row is taken by Tab, so a draft that starts a key name still steps); with onDraft every keystroke is reported; with onEscape, Esc closes the suggestions and then clears the draft and calls it
 * @avoid one free-text value (Field) · one choice among options (Select) · owning a grammar · editing a chip in place
 * @states rest · focused with suggestions · moved · faulted · chips · a refused chip
 * @madeof IconButton · Notice
 */
export function ChipBox<C>({
  chips,
  onChange,
  grammar,
  refused = never,
  placeholder,
  chippedPlaceholder = placeholder,
  initialDraft = "",
  label,
  onDraft,
  onEnter,
  onEscape,
  className = "",
  handle,
}: ChipBoxProps<C>) {
  const [draft, setDraft] = useState(initialDraft);
  const [open, setOpen] = useState(false);
  const [moved, setMoved] = useState<number | null>(null);
  const [fault, setFault] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const rowRefs = useRef<(HTMLLIElement | null)[]>([]);
  const listId = useId();

  useImperativeHandle(handle, () => ({ focus: () => input.current?.focus() }), []);

  const rows = open && fault === null ? grammar.suggest(draft) : [];
  const active = moved === null ? Math.max(0, rows.findIndex((row) => row.kind !== "typed")) : Math.min(moved, rows.length - 1);

  useEffect(() => {
    rowRefs.current[active]?.scrollIntoView?.({ block: "nearest" });
  }, [active]);

  const restart = (text: string) => {
    setDraft(text);
    setFault(null);
    setMoved(null);
    onDraft?.(text);
  };

  const accept = (text: string) => {
    const parsed = grammar.parse(text);
    if ("fault" in parsed) {
      setFault(parsed.fault);
      setOpen(true);
      return;
    }
    if (!("chip" in parsed)) return;
    restart("");
    const made = grammar.label(parsed.chip).text;
    if (!chips.some((chip) => grammar.label(chip).text === made)) onChange([...chips, parsed.chip]);
  };

  const take = (row: BoxSuggestion) => (row.kind === "key" ? restart(row.text) : accept(row.text));

  const remove = (gone: C) => {
    onChange(chips.filter((chip) => chip !== gone));
    input.current?.focus();
  };

  const expanded = open && (fault !== null || rows.length > 0);

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
        if (highlighted && (!onEnter || moved !== null || highlighted.kind !== "key")) take(highlighted);
        else if (onEnter) onEnter(event.shiftKey);
        else if (draft.trim()) accept(draft);
        return;
      case " ":
        if (!draft.trim() || grammar.hasOpenQuote(draft)) return;
        event.preventDefault();
        accept(draft);
        return;
      case "Escape":
        if (!onEscape || expanded) {
          setOpen(false);
          setMoved(null);
          if (onEscape) event.preventDefault();
          return;
        }
        event.preventDefault();
        restart("");
        onEscape();
        return;
      case "Backspace":
        if (draft !== "" || chips.length === 0) return;
        event.preventDefault();
        onChange(chips.slice(0, -1));
        return;
    }
  };

  return (
    <div
      onMouseDown={(event) => {
        if (event.target !== event.currentTarget) return;
        event.preventDefault();
        input.current?.focus();
      }}
      className={`flex flex-wrap items-center gap-chrome-tight rounded-button border border-hairline bg-canvas p-chrome-tight transition-colors hover:border-ink-mute focus-within:border-ink-mute ${className}`}
    >
      {chips.map((chip) => {
        const face = grammar.label(chip);
        const reason = refused(chip);
        return <ChipView key={face.text} face={face} reason={reason} refused={reason !== null} onRemove={() => remove(chip)} />;
      })}
      <div className="relative min-w-field-sm flex-1">
        <input
          ref={input}
          type="text"
          role="combobox"
          aria-label={label}
          aria-autocomplete="list"
          aria-expanded={expanded}
          aria-controls={listId}
          aria-activedescendant={rows.length > 0 ? `${listId}-${active}` : undefined}
          aria-invalid={fault !== null || undefined}
          spellCheck={false}
          autoComplete="off"
          value={draft}
          placeholder={chips.length > 0 ? chippedPlaceholder : placeholder}
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
                    <span className="font-mono">{row.face ?? row.text}</span>
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
