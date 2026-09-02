import { json } from "@codemirror/lang-json";
import { TREE_MAX } from "../body";
import { SourceSurface } from "./SourceSurface";
import { TextRenderer } from "./TextRenderer";

const language = json();

function parses(text: string): boolean {
  try {
    JSON.parse(text);
    return true;
  } catch {
    return false;
  }
}

/**
 * JsonRenderer — a JSON body as its copy-exact pretty-printed source, navigable and foldable.
 * @tier T2 — owns the parse check, the over-cap policy, and the as-is fallback that guarantees the pane is never a parse error
 * @anatomy the pretty-printed source (the API pretty-prints server-side, decode.rs) in a read-only CodeMirror 6 surface: cursor + keyboard navigation, fold gutter on objects and arrays, the three muted tokens key / string / number with punctuation mute · selecting or copying anywhere yields valid JSON verbatim
 * @use a body dispatched to "json" — application/json and *+json
 * @avoid a lossy tree form: key: value lines without braces, quotes and commas are not valid JSON when copied (DEC-01) · pretty-printing client-side — the document must stay the exact bytes served · showing a parse error instead of the body
 * @states rendered · as-is fallback (unparseable, or over TREE_MAX → TextRenderer, no tokens and no surface) · rendered+cut via that fallback's own cap
 * @madeof SourceSurface (CodeMirror 6, read-only) · TextRenderer (fallback)
 */
export function JsonRenderer({ text }: { text: string }) {
  if (text.length > TREE_MAX || !parses(text)) return <TextRenderer text={text} />;

  return <SourceSurface text={text} language={language} testId="body-json" />;
}
