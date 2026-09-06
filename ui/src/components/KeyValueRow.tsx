import { useLayoutEffect, useRef, useState } from "react";
import { Button } from "./Button";
import { CopyText } from "./CopyText";

/**
 * KeyValueRow — one wire key/value pair; muted name, value is a CopyText (click copies whole, selection stays native).
 * @tier T1 — display + value click-copy via CopyText; no business logic
 * @anatomy two-column grid (display:contents inside parent): name (muted, nowrap, mono, inert) · value (CopyText: body tone, break-all, mono, never truncated) · collapsible: a value taller than three lines is clamped to them, with a more / less toggle under the name; the clamp is a view of the verbatim value (click still copies the whole of it, and the confirm tones the visible lines), expanded by one act
 * @use any wire key/value line: request/response headers, trailers, decoded form fields · collapsible where a collection of long values (cookies, tokens, policies) would otherwise bury its neighbours
 * @avoid editable forms (that is Field) · data needing sort/filter · standalone full-value fields with no name column (bare CopyText) · truncating the value itself — the clamp hides lines, never characters, and always offers the rest
 * @states rest · hover/focus/confirming on the value cell (CopyText's states) · drag-selecting (native; the guard fires no copy) · collapsed (three lines, "more" under the name) · expanded ("less") — only when collapsible and the value really overflows
 * @madeof CopyText (value cell) · Button (the more / less toggle, frame="bare")
 */
export function KeyValueRow({
  name,
  value,
  collapsible = false,
}: {
  name: string;
  value: string;
  collapsible?: boolean;
}) {
  const cell = useRef<HTMLElement>(null);
  const [open, setOpen] = useState(false);
  const [overflows, setOverflows] = useState(false);

  useLayoutEffect(() => {
    setOpen(false);
  }, [value]);

  useLayoutEffect(() => {
    if (!collapsible || open) return;
    const dd = cell.current;
    if (dd) setOverflows(dd.scrollHeight > dd.clientHeight);
  }, [collapsible, open, value]);

  const clamped = collapsible && !open;
  const toggle = collapsible && (overflows || open);

  return (
    <div className="contents">
      <dt className="whitespace-nowrap font-mono text-body text-ink-mute">
        {name}
        {toggle && (
          <div>
            <Button frame="bare" tone="mute" onClick={() => setOpen(!open)} label={`${open ? "less" : "more"} of ${name}`}>
              {open ? "less" : "more"}
            </Button>
          </div>
        )}
      </dt>
      <dd ref={cell} className={clamped ? "-m-control-y max-h-value-clamp overflow-hidden p-control-y" : ""}>
        <CopyText value={value} label={name} />
      </dd>
    </div>
  );
}
