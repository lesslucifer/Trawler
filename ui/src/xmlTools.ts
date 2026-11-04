type TokenKind = "open" | "close" | "empty" | "leaf" | "text";

interface Token {
  kind: TokenKind;
  from: number;
  to: number;
  name: string;
}

export type XmlReindent = { ok: true; text: string } | { ok: false; line: number; column: number; reason: string };

export const REINDENT_STEP = "re-indented";

const VOID = new Set(["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"]);
const RAW_TEXT = new Set(["script", "style"]);
const NAME = /[^\s/>]+/y;
const DEFAULT_UNIT = "  ";
const INDENTED_TAG = /[\r\n]([ \t]+)</;

const LEAVES: { open: string; close: string; what: string }[] = [
  { open: "<!--", close: "-->", what: "a comment" },
  { open: "<![CDATA[", close: "]]>", what: "a CDATA section" },
  { open: "<?", close: "?>", what: "a processing instruction" },
];

class Malformed extends Error {
  constructor(
    readonly at: number,
    reason: string,
  ) {
    super(reason);
  }
}

function nameAt(text: string, at: number): string {
  NAME.lastIndex = at;
  return NAME.exec(text)?.[0] ?? "";
}

function tagEnd(text: string, from: number): number {
  let quote = "";
  for (let i = from; i < text.length; i += 1) {
    const ch = text[i];
    if (quote) {
      if (ch === quote) quote = "";
    } else if (ch === '"' || ch === "'") quote = ch;
    else if (ch === ">") return i + 1;
  }
  throw new Malformed(from, "a tag that never ends");
}

function doctypeEnd(text: string, from: number): number {
  let depth = 0;
  for (let i = from; i < text.length; i += 1) {
    if (text[i] === "[") depth += 1;
    else if (text[i] === "]") depth -= 1;
    else if (text[i] === ">" && depth <= 0) return i + 1;
  }
  throw new Malformed(from, "a doctype that never ends");
}

function rawTextEnd(text: string, from: number, name: string): number {
  const close = text.toLowerCase().indexOf(`</${name}`, from);
  return close < 0 ? text.length : close;
}

function tokenise(text: string): Token[] {
  const tokens: Token[] = [];
  let at = 0;
  while (at < text.length) {
    if (text[at] !== "<") {
      const next = text.indexOf("<", at);
      const to = next < 0 ? text.length : next;
      tokens.push({ kind: "text", from: at, to, name: "" });
      at = to;
      continue;
    }
    const leaf = LEAVES.find((candidate) => text.startsWith(candidate.open, at));
    if (leaf) {
      const close = text.indexOf(leaf.close, at + leaf.open.length);
      if (close < 0) throw new Malformed(at, `${leaf.what} that never ends`);
      const to = close + leaf.close.length;
      tokens.push({ kind: leaf.open === "<![CDATA[" ? "text" : "leaf", from: at, to, name: "" });
      at = to;
      continue;
    }
    if (text.startsWith("<!", at)) {
      const to = doctypeEnd(text, at);
      tokens.push({ kind: "leaf", from: at, to, name: "" });
      at = to;
      continue;
    }
    const closing = text[at + 1] === "/";
    const name = nameAt(text, at + (closing ? 2 : 1));
    if (!name) throw new Malformed(at, "a < that opens no tag");
    const to = tagEnd(text, at);
    const lower = name.toLowerCase();
    if (closing) tokens.push({ kind: "close", from: at, to, name });
    else if (text[to - 2] === "/") tokens.push({ kind: "empty", from: at, to, name });
    else if (VOID.has(lower)) tokens.push({ kind: "empty", from: at, to, name });
    else tokens.push({ kind: "open", from: at, to, name });
    at = to;
    if (!closing && RAW_TEXT.has(lower) && tokens[tokens.length - 1].kind === "open") {
      const end = rawTextEnd(text, at, lower);
      if (end > at) tokens.push({ kind: "text", from: at, to: end, name: "" });
      at = end;
    }
  }
  return tokens;
}

const sameName = (a: string, b: string) => a === b || a.toLowerCase() === b.toLowerCase();
const blank = (text: string, token: Token) => token.kind === "text" && text.slice(token.from, token.to).trim() === "";

interface Element {
  open: number;
  hasText: boolean;
  onlyBlank: boolean;
}

function matchElements(text: string, tokens: Token[]): { closes: Map<number, number>; inline: Set<number>; topText: boolean } {
  const closes = new Map<number, number>();
  const inline = new Set<number>();
  const open: Element[] = [];
  let topText = false;
  tokens.forEach((token, i) => {
    const parent = open[open.length - 1];
    if (token.kind !== "close" && parent && !blank(text, token)) parent.onlyBlank = false;
    if (token.kind === "text" && !blank(text, token)) {
      if (parent) parent.hasText = true;
      else topText = true;
    } else if (token.kind === "open") {
      open.push({ open: i, hasText: false, onlyBlank: true });
    } else if (token.kind === "close") {
      if (!parent) {
        if (VOID.has(token.name.toLowerCase())) token.kind = "empty";
        else throw new Malformed(token.from, `</${token.name}> closes nothing`);
        return;
      }
      const opened = tokens[parent.open];
      if (!sameName(opened.name, token.name)) {
        if (VOID.has(token.name.toLowerCase())) {
          token.kind = "empty";
          parent.onlyBlank = false;
          return;
        }
        throw new Malformed(token.from, `</${token.name}> closes <${opened.name}>`);
      }
      open.pop();
      closes.set(parent.open, i);
      if (parent.hasText || parent.onlyBlank) inline.add(parent.open);
    }
  });
  const unclosed = open[open.length - 1];
  if (unclosed) throw new Malformed(tokens[unclosed.open].from, `<${tokens[unclosed.open].name}> is never closed`);
  return { closes, inline, topText };
}

function lineColumn(text: string, at: number): { line: number; column: number } {
  let line = 1;
  let start = 0;
  for (let i = 0; i < at; i += 1) {
    const code = text.charCodeAt(i);
    if (code === 0x0d && text.charCodeAt(i + 1) === 0x0a) continue;
    if (code === 0x0a || code === 0x0d) {
      line += 1;
      start = i + 1;
    }
  }
  return { line, column: at - start + 1 };
}

const BREAK_AT_END = /(?:\r\n|\r|\n)$/;

export function reindentXml(source: string, lineBreak = "\n"): XmlReindent {
  const bom = source.startsWith("﻿") ? "﻿" : "";
  const text = source.slice(bom.length);
  let tokens: Token[];
  let matched: ReturnType<typeof matchElements>;
  try {
    tokens = tokenise(text);
    matched = matchElements(text, tokens);
  } catch (error) {
    if (!(error instanceof Malformed)) throw error;
    return { ok: false, ...lineColumn(text, error.at), reason: error.message };
  }
  if (matched.topText) return { ok: true, text: source };
  const unit = INDENTED_TAG.exec(text)?.[1] ?? DEFAULT_UNIT;
  const lines: string[] = [];
  let depth = 0;
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (token.kind === "text") continue;
    if (token.kind === "close") depth -= 1;
    const pad = unit.repeat(depth);
    if (token.kind === "open" && matched.inline.has(i)) {
      const close = matched.closes.get(i) ?? i;
      const inner = text.slice(token.to, tokens[close].from);
      const kept = inner.trim() === "" ? "" : inner;
      lines.push(pad + text.slice(token.from, token.to) + kept + text.slice(tokens[close].from, tokens[close].to));
      i = close;
      continue;
    }
    lines.push(pad + text.slice(token.from, token.to));
    if (token.kind === "open") depth += 1;
  }
  return { ok: true, text: bom + lines.join(lineBreak) + (BREAK_AT_END.test(text) ? lineBreak : "") };
}
