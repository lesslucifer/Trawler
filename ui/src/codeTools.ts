export type CodeKind = "javascript" | "css";
export type CodeReformat = { ok: true; text: string } | { ok: false; reason: string };

export const REFORMAT_STEP = "reformatted";

export type Beautifier = Pick<typeof import("js-beautify"), "js" | "css">;

const WHITESPACE = /\s+/g;
const BREAK_AT_END = /(?:\r\n|\r|\n)$/;
const INDENT_SIZE = 2;

let loading: Promise<Beautifier> | null = null;

export function loadBeautifier(): Promise<Beautifier> {
  loading ??= import("js-beautify").then(
    (module) => {
      const { js, css } = module.default ?? module;
      return { js, css };
    },
    (error: unknown) => {
      loading = null;
      throw error;
    },
  );
  return loading;
}

export function reformatCode(beautifier: Beautifier, kind: CodeKind, source: string, lineBreak = "\n"): CodeReformat {
  if (source.trim() === "") return { ok: false, reason: "there is nothing to reformat" };
  const options = { indent_size: INDENT_SIZE, eol: lineBreak, end_with_newline: BREAK_AT_END.test(source) };
  const text = (kind === "css" ? beautifier.css : beautifier.js)(source, options);
  if (text.replace(WHITESPACE, "") !== source.replace(WHITESPACE, "")) {
    return { ok: false, reason: "the formatter would change more than whitespace" };
  }
  return { ok: true, text };
}
