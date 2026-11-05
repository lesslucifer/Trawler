import { CopyText } from "./CopyText";

/**
 * KeyValueRow — one wire key/value pair; muted name, value is a one-line CopyText (click copies whole, selection stays native).
 * @tier T1 — display + value click-copy via CopyText; no business logic
 * @anatomy two-column grid (display:contents inside parent): name (muted, nowrap, mono, inert, never cut — the column is as wide as the longest name) · value (CopyText's one-line form: body tone, mono, cut at the end with an ellipsis when wider than its column, DEC-78)
 * @use any wire key/value line whose whole is one act away in a data window: request/response headers (the Headers section's open all), trailers, decoded form fields
 * @avoid editable forms (that is Field) · data needing sort/filter · standalone full-value fields with no name column (bare CopyText) · a collection with no data-window route to its whole values — the cut is earned by the route (DEC-78 rule 5)
 * @states rest · cut (an ellipsis shows) · hover/focus/confirming on the value cell (CopyText's states) · drag-selecting (native; the guard fires no copy)
 * @madeof CopyText (value cell, oneLine)
 */
export function KeyValueRow({ name, value }: { name: string; value: string }) {
  return (
    <div className="contents">
      <dt className="whitespace-nowrap font-mono text-body text-ink-mute">{name}</dt>
      <dd className="min-w-0">
        <CopyText value={value} label={name} oneLine />
      </dd>
    </div>
  );
}
