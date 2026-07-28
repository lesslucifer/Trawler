import { xml } from "@codemirror/lang-xml";
import { TEXT_MAX } from "../body";
import { SourceSurface } from "./SourceSurface";
import { TextRenderer } from "./TextRenderer";

const language = xml();

/**
 * XmlRenderer — an XML or HTML body as indented source, navigable and foldable by element.
 * @tier T2 — owns the over-cap policy; the indentation is the API's (decode.rs pretty-prints with quick-xml when decode=1)
 * @anatomy the pretty-printed source in the same read-only CodeMirror 6 surface JsonRenderer uses: element folding via the fold gutter, cursor + keyboard navigation, tag names in the key token, attribute values in the string token, attribute names mute, element text body tone
 * @use a body dispatched to "xml" — application/xml, text/xml, text/html, *+xml
 * @avoid re-indenting client-side — the server's pretty-print is the source of truth · a hand-built DOM-parse tree: folding the source covers the same want without a second document (DEC-02)
 * @states rendered · over TEXT_MAX → TextRenderer with its cut marker
 * @madeof SourceSurface (CodeMirror 6, read-only) · TextRenderer (over cap)
 */
export function XmlRenderer({ text }: { text: string }) {
  if (text.length > TEXT_MAX) return <TextRenderer text={text} />;

  return <SourceSurface text={text} language={language} testId="body-xml" />;
}
