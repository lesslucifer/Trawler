import { HighlightStyle } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags } from "@lezer/highlight";

// The three DEC-11 body tokens, and only those three — a full syntax theme is
// what the family's @avoid forbids. Classes rather than values, so the palette
// is the single source: a hex here is a second copy of a token, and the copy is
// what drifted when W6 step ① deleted the tokens this comment claimed to mirror.
export const warpHighlight = HighlightStyle.define([
  { tag: [tags.propertyName, tags.tagName], class: "text-syn-key" },
  { tag: [tags.string, tags.attributeValue], class: "text-syn-str" },
  { tag: tags.number, class: "text-syn-num" },
  { tag: [tags.punctuation, tags.bracket, tags.attributeName], class: "text-ink-mute" },
]);

export const warpTheme = EditorView.theme(
  {
    "&": { backgroundColor: "transparent", color: "#c9c0ad", fontSize: "12px" },
    ".cm-content": { fontFamily: '"DM Mono", ui-monospace, monospace', padding: "0" },
    ".cm-cursor": { borderLeftColor: "#f7f5f0" },
    ".cm-gutters": {
      backgroundColor: "transparent",
      border: "none",
      color: "#aea69c",
    },
    ".cm-foldPlaceholder": {
      backgroundColor: "#383330",
      border: "1px solid #3f3a36",
      borderRadius: "3px",
      color: "#aea69c",
    },
    "&.cm-focused": { outline: "none" },
  },
  { dark: true },
);
