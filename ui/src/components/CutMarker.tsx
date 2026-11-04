import { createContext, useContext } from "react";
import { humanBytePair } from "../body";
import { Button } from "./Button";

export interface OpenWholeAct {
  open: () => void;
}

export const OpenWhole = createContext<OpenWholeAct | null>(null);

/**
 * CutMarker — the truncation statement for BodyViewer's renderer family: what is shown, of how much, and the way out.
 * @tier T2 — owns the family's never-truncate-silently rule and the human-readable size form (B · KiB · MiB, one decimal below 10); reads the OpenWhole context and never holds an opener of its own
 * @anatomy a warn-tone line above a hairline rule: "⚠ showing N of M", both sizes human-readable — and never rounded to the same string, which would read as no truncation at all — then a Button "open in data window" on the same line, wrapping under it in a narrow pane, wherever an OpenWhole opener is provided; with none the line ends "— save to see all"
 * @use from a family member whose rendered output is capped, appended after the last visible line; BodyViewer provides the OpenWhole context around its dispatch, so no member threads an opener through its props
 * @avoid rendering it when the whole body fits (a marker that is always there says nothing) · raw byte counts · a bare ellipsis, which is easy to miss (DEC-12) · a Button with no opener behind it (DEC-21) · warn-toning the Button (Button's own @avoid)
 * @states shown, the data window as the way out · shown, save as the way out (no opener) — a member that is not capped renders no marker at all
 * @madeof Button (T1) · the OpenWhole context, read and never held
 */
export function CutMarker({ shown, total }: { shown: number; total: number }) {
  const [shownSize, totalSize] = humanBytePair(shown, total);
  const opener = useContext(OpenWhole);

  return (
    <span className="mt-chrome-gap flex flex-wrap items-center gap-2 border-t border-hairline pt-chrome-y font-ui text-chrome text-warn">
      <span>
        ⚠ showing {shownSize} of {totalSize}
        {opener ? "" : " — save to see all"}
      </span>
      {opener && <Button onClick={opener.open}>open in data window</Button>}
    </span>
  );
}
