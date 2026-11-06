import { useMemo } from "react";
import { byteLength, formCut, TEXT_MAX } from "../body";
import { labelShape, readForm } from "../formPairs";
import { CutMarker } from "./CutMarker";
import { KeyValueRow } from "./KeyValueRow";

/**
 * FormRenderer — a form body decoded into its key/value pairs: URL-encoded pairs, name: value lines, or a URL's parts then its query's pairs.
 * @tier T2 — owns the pair-count cap; the shape comes from the served label and the decoding is formPairs' readForm, the data window's own
 * @anatomy KeyValueRow× in a two-column grid this component supplies (KeyValueRow is display:contents, so column sizing is the parent's job): key mute and nowrap, value a CopyText — click copies the whole value, drag-select stays native · 4px row gap · a CutMarker when the raw text exceeds TEXT_MAX
 * @use a body dispatched to "form" — application/x-www-form-urlencoded, text/rfc822-headers, text/uri-list — and anywhere URL-encoded pairs must be presented
 * @avoid an inline <dl> that re-draws the pair: the value cell's click-to-copy is KeyValueRow's, not this member's · showing the raw encoded string when the decoded pairs are what the user reads
 * @states rendered · rendered+cut (over TEXT_MAX) — no empty state; a zero-byte body never reaches a member
 * @madeof KeyValueRow×
 */
export function FormRenderer({ text, label = null }: { text: string; label?: string | null }) {
  // Cut on a pair boundary, never at a character offset into the raw text: an
  // offset can land mid-%XX or inside an escape run, and the half-decoded value
  // — a stray "%E" or U+FFFD — is what CopyText hands to the clipboard.
  const shape = labelShape(label) ?? "query";
  const cut = text.length > TEXT_MAX;
  const shown = cut ? text.slice(0, formCut(text, TEXT_MAX, shape)) : text;
  const entries = useMemo(() => {
    const { parts, pairs } = readForm(shown, shape);
    return [...parts, ...pairs].map((row): [string, string] => [row.key, row.value]);
  }, [shown, shape]);

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
