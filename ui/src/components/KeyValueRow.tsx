import { CopyText } from "./CopyText";

/**
 * KeyValueRow — one wire key/value pair; muted name, value is a CopyText (click copies whole, selection stays native).
 * @tier T1 — display + value click-copy via CopyText; no business logic
 * @anatomy two-column grid (display:contents inside parent): name (muted, nowrap, mono, inert) · value (CopyText: body tone, break-all, mono, never truncated)
 * @use any wire key/value line: request/response headers, trailers, decoded form fields
 * @avoid editable forms (that is Field) · data needing sort/filter · standalone full-value fields with no name column (bare CopyText)
 * @states rest · hover/focus/confirming on the value cell (CopyText's states) · drag-selecting (native; the guard fires no copy)
 * @madeof CopyText (value cell)
 */
export function KeyValueRow({ name, value }: { name: string; value: string }) {
  return (
    <div className="contents">
      <dt className="whitespace-nowrap font-mono text-body text-ink-mute">{name}</dt>
      <dd>
        <CopyText value={value} label={name} />
      </dd>
    </div>
  );
}
