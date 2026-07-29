import { humanBytePair } from "../body";

/**
 * CutMarker — the BodyRenderer family's truncation statement: what is shown, of how much, and the way out.
 * @tier T2 — owns the family's never-truncate-silently rule and the human-readable size form (B · KiB · MiB, one decimal below 10)
 * @anatomy a warn-tone line above a hairline rule: "⚠ showing N of M — save to see all", both sizes human-readable — and never rounded to the same string, which would read as no truncation at all
 * @use from a family member whose rendered output is capped, appended after the last visible line
 * @avoid rendering it when the whole body fits (a marker that is always there says nothing) · raw byte counts · a bare ellipsis, which is easy to miss (DEC-12)
 * @states shown — the only state; a member that is not capped renders no marker at all
 * @madeof —
 */
export function CutMarker({ shown, total }: { shown: number; total: number }) {
  const [shownSize, totalSize] = humanBytePair(shown, total);

  return (
    <span className="mt-1.5 block border-t border-hairline pt-1 font-ui text-chrome text-warn">
      ⚠ showing {shownSize} of {totalSize} — save to see all
    </span>
  );
}
