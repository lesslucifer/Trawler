import type { TextRange } from "./components/index";

export interface JsonScalar extends TextRange {
  type: "string" | "number" | "literal";
  raw: string;
}

export interface JsonMember {
  key: string;
  keyRaw: string;
  from: number;
  node: JsonNode;
}

export interface JsonObject extends TextRange {
  type: "object";
  members: JsonMember[];
}

export interface JsonArray extends TextRange {
  type: "array";
  items: JsonNode[];
}

export type JsonNode = JsonScalar | JsonObject | JsonArray;

export type JsonParse =
  | { ok: true; root: JsonNode; bom: boolean }
  | { ok: false; line: number; column: number; reason: string };

export type Indent = "2" | "4" | "tab" | "min";
export type PathSegment = string | number;

export const INDENTS: { id: Indent; label: string; step: string }[] = [
  { id: "2", label: "2", step: "indent 2 spaces" },
  { id: "4", label: "4", step: "indent 4 spaces" },
  { id: "tab", label: "tab", step: "indent a tab" },
  { id: "min", label: "minified", step: "minified" },
];
export const SORT_STEP = "sorted keys";

export const INDENT_UNIT: Record<Indent, string | null> = { "2": "  ", "4": "    ", tab: "\t", min: null };
const NUMBER = /-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/y;
const LITERALS = ["true", "false", "null"];
const ESCAPES = new Set(['"', "\\", "/", "b", "f", "n", "r", "t"]);
const HEX4 = /^[0-9a-fA-F]{4}$/;
const IDENTIFIER = /^[A-Za-z_$][\w$]*$/;
const BOM = 0xfeff;

class Malformed {
  constructor(
    readonly at: number,
    readonly reason: string,
  ) {}
}

interface Open {
  node: JsonObject | JsonArray;
  key: Omit<JsonMember, "node"> | null;
}

const isSpace = (code: number) => code === 0x20 || code === 0x09 || code === 0x0a || code === 0x0d;

function lineAndColumn(text: string, at: number): { line: number; column: number } {
  let line = 1;
  let start = 0;
  for (let i = 0; i < at; i += 1) {
    const code = text.charCodeAt(i);
    if (code === 0x0d && text.charCodeAt(i + 1) === 0x0a) i += 1;
    if (code === 0x0d || code === 0x0a) {
      line += 1;
      start = i + 1;
    }
  }
  return { line, column: at - start + 1 };
}

function parseText(text: string, bom: boolean): JsonNode {
  let i = bom ? 1 : 0;
  const stack: Open[] = [];
  const parsed: { root: JsonNode | null } = { root: null };

  function fail(reason: string): never {
    throw new Malformed(i, i >= text.length ? "the text ends before the JSON does" : reason);
  }
  const skip = () => {
    while (i < text.length && isSpace(text.charCodeAt(i))) i += 1;
  };
  const attach = (node: JsonNode) => {
    const open = stack[stack.length - 1];
    if (!open) parsed.root = node;
    else if (open.node.type === "array") open.node.items.push(node);
    else if (open.key) open.node.members.push({ ...open.key, node });
  };

  const readString = (): string => {
    const from = i;
    i += 1;
    for (;;) {
      if (i >= text.length) throw new Malformed(from, "a string is not closed");
      const code = text.charCodeAt(i);
      if (code === 0x22) {
        i += 1;
        return text.slice(from, i);
      }
      if (code === 0x5c) {
        const escape = text[i + 1];
        if (escape === undefined) i += 1;
        if (escape === "u") {
          if (!HEX4.test(text.slice(i + 2, i + 6))) fail("\\u needs four hex digits");
          i += 6;
        } else {
          if (!ESCAPES.has(escape)) fail(`\\${escape} is not an escape JSON knows`);
          i += 2;
        }
      } else if (code < 0x20) {
        fail("a control character or line break inside a string");
      } else {
        i += 1;
      }
    }
  };

  const readKey = (open: Open) => {
    skip();
    if (text[i] !== '"') fail("expected a key in double quotes");
    const from = i;
    const keyRaw = readString();
    skip();
    if (text[i] !== ":") fail("expected : after the key");
    i += 1;
    open.key = { key: JSON.parse(keyRaw) as string, keyRaw, from };
  };

  const readScalar = (): JsonScalar => {
    const from = i;
    const c = text[i];
    if (c === '"') return { type: "string", raw: readString(), from, to: i };
    if (c === "-" || (c >= "0" && c <= "9")) {
      NUMBER.lastIndex = i;
      const found = NUMBER.exec(text);
      if (!found) fail("a number is not written the way JSON writes numbers");
      i += found[0].length;
      return { type: "number", raw: found[0], from, to: i };
    }
    const literal = LITERALS.find((word) => text.startsWith(word, i));
    if (!literal) fail("expected a value: an object, array, string, number, true, false or null");
    i += literal.length;
    return { type: "literal", raw: literal, from, to: i };
  };

  let wantValue = true;
  for (;;) {
    skip();
    if (wantValue) {
      const c = text[i];
      if (c === "{" || c === "[") {
        const node: JsonObject | JsonArray =
          c === "{" ? { type: "object", from: i, to: i, members: [] } : { type: "array", from: i, to: i, items: [] };
        i += 1;
        skip();
        if (text[i] === (c === "{" ? "}" : "]")) {
          i += 1;
          node.to = i;
          attach(node);
          wantValue = false;
          continue;
        }
        const open: Open = { node, key: null };
        stack.push(open);
        if (node.type === "object") readKey(open);
        continue;
      }
      attach(readScalar());
      wantValue = false;
      continue;
    }
    const open = stack[stack.length - 1];
    if (!open) {
      if (i < text.length) fail("there is more text after the JSON ends");
      if (!parsed.root) fail("expected a value");
      return parsed.root;
    }
    const close = open.node.type === "object" ? "}" : "]";
    if (text[i] === ",") {
      i += 1;
      if (open.node.type === "object") readKey(open);
      wantValue = true;
    } else if (text[i] === close) {
      i += 1;
      open.node.to = i;
      stack.pop();
      attach(open.node);
    } else {
      fail(`expected , or ${close}`);
    }
  }
}

export function parseJson(text: string): JsonParse {
  const bom = text.charCodeAt(0) === BOM;
  try {
    return { ok: true, root: parseText(text, bom), bom };
  } catch (error) {
    if (!(error instanceof Malformed)) throw error;
    return { ok: false, ...lineAndColumn(text, error.at), reason: error.reason };
  }
}

interface Entry {
  keyRaw?: string;
  node: JsonNode;
}

interface Frame {
  entries: Entry[];
  at: number;
  depth: number;
  close: string;
}

const byKey = (a: JsonMember, b: JsonMember) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0);

export function layoutJson(root: JsonNode, unit: string | null, sort: boolean, lineBreak: string): string {
  const out: string[] = [];
  const breakAt = (depth: number) => {
    if (unit !== null) out.push(lineBreak, unit.repeat(depth));
  };
  const open = (node: JsonNode, depth: number): Frame | null => {
    if (node.type !== "object" && node.type !== "array") {
      out.push(node.raw);
      return null;
    }
    const entries: Entry[] =
      node.type === "array" ? node.items.map((item) => ({ node: item })) : sort ? [...node.members].sort(byKey) : node.members;
    const [start, close] = node.type === "array" ? ["[", "]"] : ["{", "}"];
    if (entries.length === 0) {
      out.push(start, close);
      return null;
    }
    out.push(start);
    return { entries, at: 0, depth: depth + 1, close };
  };

  const stack: Frame[] = [];
  const first = open(root, 0);
  if (first) stack.push(first);
  while (stack.length) {
    const frame = stack[stack.length - 1];
    if (frame.at === frame.entries.length) {
      stack.pop();
      breakAt(frame.depth - 1);
      out.push(frame.close);
      continue;
    }
    if (frame.at > 0) out.push(",");
    breakAt(frame.depth);
    const entry = frame.entries[frame.at];
    frame.at += 1;
    if (entry.keyRaw !== undefined) out.push(entry.keyRaw, unit === null ? ":" : ": ");
    const inner = open(entry.node, frame.depth);
    if (inner) stack.push(inner);
  }
  return out.join("");
}

const endsWithBreak = (text: string) => /[\r\n]$/.test(text);

export type Reformat = { ok: true; text: string } | Extract<JsonParse, { ok: false }>;

export function reformatJson(text: string, unit: string | null, sort: boolean, lineBreak: string): Reformat {
  const parsed = parseJson(text);
  if (!parsed.ok) return parsed;
  const body = layoutJson(parsed.root, unit, sort, lineBreak);
  return { ok: true, text: `${parsed.bom ? "\uFEFF" : ""}${body}${endsWithBreak(text) ? lineBreak : ""}` };
}

export function indentOf(text: string): string | null {
  const indented = /[\r\n]([ \t]*)\S/.exec(text.replace(/^\uFEFF/, "").trim());
  return indented ? indented[1] : null;
}

export function indentChosen(step: string): Indent | null {
  return INDENTS.find((choice) => choice.step === step)?.id ?? null;
}

export interface NodeAt {
  path: PathSegment[];
  node: JsonNode;
}

const contains = (range: TextRange, offset: number) => range.from <= offset && offset <= range.to;

export function nodeAt(root: JsonNode, offset: number): NodeAt {
  const path: PathSegment[] = [];
  let node = root;
  for (;;) {
    if (node.type === "object") {
      const member = node.members.find((m) => contains({ from: m.from, to: m.node.to }, offset));
      if (!member) break;
      path.push(member.key);
      node = member.node;
    } else if (node.type === "array") {
      const index = node.items.findIndex((item) => contains(item, offset));
      if (index < 0) break;
      path.push(index);
      node = node.items[index];
    } else {
      break;
    }
  }
  return { path, node };
}

export function pathText(path: PathSegment[]): string {
  return path.reduce<string>((text, segment) => {
    if (typeof segment === "number") return `${text}[${segment}]`;
    return IDENTIFIER.test(segment) ? `${text}.${segment}` : `${text}[${JSON.stringify(segment)}]`;
  }, "$");
}

export function valueText(text: string, node: JsonNode, lineBreak = "\n"): string {
  if (node.type === "string") return JSON.parse(node.raw) as string;
  const value = text.slice(node.from, node.to);
  return lineBreak === "\n" ? value : value.split("\n").join(lineBreak);
}

export function surfaceText(text: string): string {
  return text.replace(/\r\n?/g, "\n");
}
