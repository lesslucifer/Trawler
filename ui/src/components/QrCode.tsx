import { useMemo } from "react";
import { encode } from "uqr";

// Two light modules around the code: the plate is the rest of the quiet zone a
// camera needs to find its edge against a dark screen.
const QUIET_MODULES = 2;

function modulePath(value: string): { size: number; d: string } {
  const { data } = encode(value, { ecc: "M", border: QUIET_MODULES });
  const d = data
    .flatMap((row, y) => row.map((dark, x) => (dark ? `M${x} ${y}h1v1h-1z` : "")))
    .join("");
  return { size: data.length, d };
}

/**
 * QrCode — one value drawn as a code a phone's camera opens; an extra route to a value printed beside it, never its only one.
 * @tier T1 — value in, code out: no business logic. Which value earns a code is the screen's rule (DEC-57 rule 2), and the encoder is local and vendored (rule 3)
 * @anatomy a square light plate (ink) the size of scan-code, rounded like a button · dark modules (canvas) drawn crisp, with a two-module quiet zone inside the plate · no caption of its own: the value it encodes is the sentence beside it
 * @use a value a device must receive and cannot paste from this machine — the certificate URL a phone opens — rendered beside that value, which stays printed and copyable
 * @avoid the value's only rendering: a code is unreadable to a person, so the printed value stays (DEC-57 rule 1) · a value the reader can copy on this machine · a bundle of values: it encodes exactly the one string it is given (rule 5) · a value a scan cannot use, such as a proxy host and port a phone types into its Wi-Fi settings · decoration
 * @states rendered — no empty state (the screen omits it where there is no value) and no interaction
 * @madeof — (encoder: uqr, bundled)
 */
export function QrCode({ value, label }: { value: string; label: string }) {
  const { size, d } = useMemo(() => modulePath(value), [value]);
  return (
    <svg
      role="img"
      aria-label={`${label}, as a code to scan`}
      data-value={value}
      viewBox={`0 0 ${size} ${size}`}
      shapeRendering="crispEdges"
      className="block h-scan-code w-scan-code rounded-button bg-ink text-canvas"
    >
      <path d={d} fill="currentColor" />
    </svg>
  );
}
