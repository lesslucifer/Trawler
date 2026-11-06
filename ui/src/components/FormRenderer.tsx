import { useMemo } from "react";
import { byteLength, formPairCut, TEXT_MAX } from "../body";
import { readForm, shapeOf } from "../formPairs";
import { CutMarker } from "./CutMarker";
import { KeyValueRow } from "./KeyValueRow";
import { TextRenderer } from "./TextRenderer";

/**
 * FormRenderer — a form body decoded into its key/value pairs: URL-encoded pairs, name: value lines, or a URL's parts then its query's pairs.
 * @tier T2 — owns the pair-count cap and when a form falls back to text; the shape is formPairs' shapeOf over the served label, the decoding its readForm, the data window's own
 * @anatomy KeyValueRow× in a two-column grid this component supplies (KeyValueRow is display:contents, so column sizing is the parent's job): key mute and nowrap, value a CopyText — click copies the whole value, drag-select stays native · 4px row gap · a CutMarker when the raw text exceeds TEXT_MAX
 * @use a body dispatched to "form" — application/x-www-form-urlencoded, text/rfc822-headers, text/uri-list — and anywhere URL-encoded pairs must be presented
 * @avoid an inline <dl> that re-draws the pair: the value cell's click-to-copy is KeyValueRow's, not this member's · showing the raw encoded string when the decoded pairs are what the user reads
 * @states rendered · rendered+cut (URL-encoded pairs over TEXT_MAX) · as text (no form shape, or name: value lines or a URL over TEXT_MAX, which have no safe cut) — no empty state; a zero-byte body never reaches a member
 * @madeof KeyValueRow× · TextRenderer (T2) — the fallback
 */
export function FormRenderer({ text, label = null }: { text: string; label?: string | null }) {
  // Cut on a pair boundary, never at a character offset into the raw text: an
  // offset can land mid-%XX or inside an escape run, and the half-decoded value
  // — a stray "%E" or U+FFFD — is what CopyText hands to the clipboard.
  const shape = useMemo(() => shapeOf(text, label), [text, label]);
  const cut = text.length > TEXT_MAX;
  const asText = shape === "none" || (cut && shape !== "query");
  const shown = cut && !asText ? text.slice(0, formPairCut(text, TEXT_MAX)) : text;
  const entries = useMemo(() => {
    if (asText) return [];
    const { parts, pairs } = readForm(shown, shape);
    return [...parts, ...pairs].map((row): [string, string] => [row.key, row.value]);
  }, [asText, shown, shape]);

  if (asText) return <TextRenderer text={text} />;

  return (
    <>
      <dl
        data-testid="body-form"
        className={
          // design-allow: grid-cols-[auto_1fr] — a two-column template shape, no size in it
          "grid grid-cols-[auto_1fr] gap-x-grid-gap-x gap-y-grid-gap-y"
        }
      >
        {entries.map(([key, value], i) => (
          <KeyValueRow key={i} name={key} value={value} />
        ))}
      </dl>
      {cut && <CutMarker shown={byteLength(shown)} total={byteLength(text)} />}
    </>
  );
}
