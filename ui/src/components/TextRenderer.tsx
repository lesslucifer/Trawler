import { byteLength, TEXT_MAX } from "../body";
import { CutMarker } from "./CutMarker";

/**
 * TextRenderer — a body with no grammar, rendered verbatim: mono, wrapping, uncoloured.
 * @tier T2 — per-type render logic + the truncation policy that keeps a 10 MiB body off the DOM
 * @props text: the decoded body; size?: the body's own size in bytes when the caller already holds it — the fetch does, and re-deriving it means encoding the whole body to read a number off it. Ignored where the decode was lossy, since the two counts then measure different things
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
      <CutMarker shown={byteLength(shown)} total={total(text, shown, size)} />
    </pre>
  );
}

// `size` counts the body's own bytes; byteLength counts the decoded text's. The
// two agree only where the decode read every byte — a replacement character
// stands for one it could not, and re-encodes to three, which can push the shown
// count past the whole body's size and print "showing more than there is". A
// replacement character inside the shown slice is that signal, and the slice is
// bounded by the cap, where the body this prop exists to stop encoding is not.
function total(text: string, shown: string, size?: number): number {
  const lossy = shown.includes("\uFFFD");
  return size !== undefined && !lossy ? size : byteLength(text);
}
