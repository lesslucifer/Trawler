import { useMemo, type Ref } from "react";
import { chipLabel, hasOpenQuote, parseDraft, suggest, type Chip, type Suggestion } from "../query";
import { ChipBox, type ChipGrammar } from "./ChipBox";
import { MethodLabel } from "./MethodLabel";

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

function SuggestionText({ row }: { row: Suggestion }) {
  if (row.key !== "method" || row.kind === "key" || row.value === undefined) return <>{row.text}</>;
  return (
    <>
      <span className="text-ink-mute">{row.text.startsWith("-") ? "-" : ""}method:</span>
      <MethodLabel method={row.value} />
    </>
  );
}

function faceOf(chip: Chip) {
  const text = chipLabel(chip);
  const quoted = chip.key === "q" && text.endsWith('"');
  return {
    text,
    key: chip.key === "q" ? undefined : chip.key,
    negated: chip.negated,
    value: chip.key === "method" ? <MethodLabel method={chip.value} /> : quoted ? `"${chip.value}"` : chip.value,
    wire: chip.key === "host" ? "host" : undefined,
  };
}

/**
 * QueryBar — the flow list's filter: a ChipBox over query.ts's grammar; suggestions follow the draft, and a chip is made only when the draft parses.
 * @tier T2 — owns the grammar (draft → chip or fault, draft → suggestions) through query.ts; chips, hosts seen and the refusal are the caller's
 * @anatomy a ChipBox: Field's box, flex-wrap, chrome-tight padding · chips: wash-tint pill, mono, key and "-" ink-mute, value ink (MethodLabel for a method), IconButton × size 24 · refused: warn border + ⚠ · draft: borderless mono input, min-w-field-sm · suggestions: Select's popup surface under the draft, completion mono + hint ui + the key that takes it · fault: an inline warn Notice in the suggestions' place
 * @use the one filter of a list the user narrows by typed keys and text: the flow list's header. Enter or Tab takes the highlighted suggestion (a key goes into the draft, a value becomes a chip), Space outside quotes takes the draft as typed, ↑/↓ move the highlight without wrapping, Esc closes the suggestions, Backspace in an empty draft removes the last chip, and each chip's × removes that one. A text chip with a space is typed in quotes. An unknown key is a fault only within two edits of a real one; any other word:rest is text. The screen binds ⌘F to focus() on the handle it passes
 * @avoid one free-text value (Field) · one choice among options (Select) · actions or commands in the bar (DEC-71 rule 9) · calling the caller on a keystroke: onChange is for chips only · editing a chip in place
 * @states rest · focused (suggestions, Enter or Tab takes the highlight, Space the draft as typed) · moved (↑/↓ highlight) · faulted (warn Notice until the next keystroke) · chips · a refused chip. No disabled, loading or restored state
 * @madeof ChipBox · MethodLabel
 */
export function QueryBar({ chips, onChange, hosts, refused = never, className = "", handle }: QueryBarProps) {
  const grammar = useMemo<ChipGrammar<Chip>>(
    () => ({
      parse: parseDraft,
      suggest: (draft) => suggest(draft, hosts).map((row) => ({ ...row, face: <SuggestionText row={row} /> })),
      label: faceOf,
      hasOpenQuote,
    }),
    [hosts],
  );
  return (
    <ChipBox
      chips={chips}
      onChange={onChange}
      grammar={grammar}
      refused={(chip) => (refused(chip) ? "" : null)}
      label="filter"
      placeholder="filter — status:4xx, host:api, or any text"
      chippedPlaceholder="add a filter"
      className={className}
      handle={handle}
    />
  );
}
