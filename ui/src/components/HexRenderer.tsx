import { hexRows, HEX_MAX } from "../body";
import { CutMarker } from "./CutMarker";

/**
 * HexRenderer — raw bytes as an offset · hex · ASCII dump.
 * @tier T2 — owns the dump format and the 4 KiB render cap (a capture-cap body would be ~600k DOM lines)
 * @anatomy its own <pre> surface (mono, wrapping): offset in 8 hex digits mute · 16 space-separated bytes body tone · pipe-delimited ASCII gutter, non-printable as "." · a CutMarker when the body exceeds HEX_MAX
 * @use a body dispatched to "hex" — anything not json / xml / image / form / text — and every raw-mode body that is not valid UTF-8 or carries control characters
 * @avoid non-body binary data (there is no such screen) · the muted "… N more bytes" ellipsis hexDump appends by default: it is easy to miss, and the family states truncation in warn tone with the total (DEC-12)
 * @states rendered · rendered+cut (over HEX_MAX)
 * @madeof —
 */
export function HexRenderer({ bytes }: { bytes: Uint8Array }) {
  const shown = Math.min(bytes.length, HEX_MAX);

  return (
    <pre
      data-testid="body-hex"
      className="overflow-x-auto whitespace-pre font-mono text-body text-ink-body"
    >
      {hexRows(bytes).map((row, i) => (
        <span key={i}>
          <span className="text-ink-mute">{row.offset}</span>
          {"  "}
          {row.hex}
          {"  "}
          <span className="text-ink">{row.ascii}</span>
          {"\n"}
        </span>
      ))}
      {bytes.length > HEX_MAX && <CutMarker shown={shown} total={bytes.length} />}
    </pre>
  );
}
