import { byteLength, TEXT_MAX } from "../body";
import { CutMarker } from "./CutMarker";

/**
 * TextRenderer — a body with no grammar, rendered verbatim: mono, wrapping, uncoloured.
 * @tier T2 — per-type render logic + the truncation policy that keeps a 10 MiB body off the DOM
 * @props text: the decoded body; size?: the body's own size in bytes — pass it only where text is a whole decode of those bytes, since a lossy decode makes the two counts measure different things. The fetch holds both, and re-deriving the size means encoding the whole body to read one number off it
 * @anatomy the text verbatim in a <pre>: mono 12px, whitespace-pre-wrap + break-all so long lines wrap instead of scrolling the panel, body ink · a CutMarker when the text exceeds TEXT_MAX · no syntax tokens, no line numbers, no fold gutter, no editor surface — the three DEC-11 tokens need a grammar to apply them to
 * @use every text body that is not the XML/HTML sub-group (text/plain, text/csv, application/javascript, …), every raw-mode readable body, and JsonRenderer's as-is fallback when a body will not parse
 * @avoid guessing a grammar it was not given — a text body that turns out to be JSON is JsonRenderer's dispatch, not this member's sniff · truncating without the marker · a CodeMirror surface: plain text has no structure to navigate or fold
 * @states rendered · rendered+cut (over TEXT_MAX) — no empty state, since a zero-byte body never reaches a member (BodyViewer shows a notice instead)
 * @madeof —
 */
export function TextRenderer({ text, size }: { text: string; size?: number }) {
  if (text.length <= TEXT_MAX) {
    return (
      <pre
        data-testid="body-text"
        className="overflow-x-auto whitespace-pre-wrap break-all font-mono text-body text-ink-body"
      >
        {text}
      </pre>
    );
  }

  const shown = text.slice(0, TEXT_MAX);
  return (
    <pre
      data-testid="body-text"
      className="overflow-x-auto whitespace-pre-wrap break-all font-mono text-body text-ink-body"
    >
      {shown}
      <CutMarker shown={byteLength(shown)} total={size ?? byteLength(text)} />
    </pre>
  );
}
