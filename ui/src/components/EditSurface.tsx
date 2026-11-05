import { useEffect, useImperativeHandle, useRef, useState, type ReactNode, type Ref } from "react";
import { createPortal } from "react-dom";
import {
  codeFolding,
  foldable,
  foldEffect,
  foldedRanges,
  foldGutter,
  foldKeymap,
  syntaxHighlighting,
  unfoldAll,
  unfoldEffect,
} from "@codemirror/language";
import { json } from "@codemirror/lang-json";
import { xml } from "@codemirror/lang-xml";
import { standardKeymap } from "@codemirror/commands";
import {
  Annotation,
  Compartment,
  EditorState,
  StateEffect,
  StateField,
  type Extension,
  type Range,
  type Transaction,
} from "@codemirror/state";
import { Decoration, EditorView, keymap, lineNumbers, WidgetType, type DecorationSet } from "@codemirror/view";
import { warpHighlight, warpTheme } from "./warpTheme";

export type SurfaceLanguage = "json" | "xml" | "plain";
export type FoldDepth = number | "all" | "none";

export interface TextRange {
  from: number;
  to: number;
}

export interface LineInset {
  afterLine: number;
  node: ReactNode;
}

export interface EditSurfaceHandle {
  foldTo: (depth: FoldDepth) => void;
  focus: () => void;
  positionAt: (point: { x: number; y: number }) => number | null;
  cursor: () => number;
  selection: () => TextRange;
  select: (range: TextRange) => void;
}

export interface EditSurfaceProps {
  text: string;
  language: SurfaceLanguage;
  readOnly?: boolean;
  wrap?: boolean;
  matches?: TextRange[];
  current?: number;
  reveal?: number;
  onChange?: (text: string, step: string, extend: boolean) => void;
  onSelect?: (text: string, line: number) => void;
  inset?: LineInset | null;
  lineBreak?: string;
  handle?: Ref<EditSurfaceHandle>;
  testId?: string;
}

const MARK = Decoration.mark({ class: "bg-wash-select" });
const CURRENT = Decoration.mark({ class: "bg-wash-active outline outline-focus outline-ink-mute" });
const GROUP_MS = 500;
const NO_MATCHES: TextRange[] = [];

const external = Annotation.define<boolean>();
const setMarks = StateEffect.define<{ matches: TextRange[]; current: number }>();
const setInset = StateEffect.define<{ line: number; host: HTMLElement } | null>();

const marks = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(set, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setMarks)) {
        const length = tr.state.doc.length;
        const ranges: Range<Decoration>[] = effect.value.matches
          .filter((m) => m.from < m.to && m.to <= length)
          .map((m, i) => (i === effect.value.current ? CURRENT : MARK).range(m.from, m.to));
        return Decoration.set(ranges, true);
      }
    }
    return set.map(tr.changes);
  },
  provide: (field) => EditorView.decorations.from(field),
});

class InsetWidget extends WidgetType {
  constructor(readonly host: HTMLElement) {
    super();
  }
  eq(other: InsetWidget) {
    return other.host === this.host;
  }
  toDOM() {
    return this.host;
  }
  ignoreEvent() {
    return true;
  }
}

const insetField = StateField.define<{ line: number; host: HTMLElement } | null>({
  create: () => null,
  update(value, tr) {
    for (const effect of tr.effects) if (effect.is(setInset)) return effect.value;
    if (!value || !tr.docChanged) return value;
    const at = tr.startState.doc.line(Math.min(value.line, tr.startState.doc.lines)).to;
    return { ...value, line: tr.state.doc.lineAt(tr.changes.mapPos(at)).number };
  },
});

const insetDecorations = EditorView.decorations.compute([insetField], (state) => {
  const value = state.field(insetField);
  if (!value) return Decoration.none;
  const line = state.doc.line(Math.min(Math.max(value.line, 1), state.doc.lines));
  return Decoration.set([
    Decoration.widget({ widget: new InsetWidget(value.host), block: true, side: 1 }).range(line.to),
  ]);
});

const surfaceTheme = EditorView.theme({
  "&": { height: "100%" },
  ".cm-scroller": { overflow: "auto", lineHeight: "1.5" },
  ".cm-lineNumbers .cm-gutterElement": { textAlign: "right", userSelect: "none" },
  ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
    backgroundColor: "rgba(174,166,156,.35)",
  },
});

function language(choice: SurfaceLanguage): Extension {
  if (choice === "json") return json();
  if (choice === "xml") return xml();
  return [];
}

function foldPlaceholder(_view: EditorView, onclick: (event: Event) => void, lines: unknown) {
  const chip = document.createElement("span");
  chip.className = "ml-2 cursor-pointer rounded-button bg-wash-tint px-chrome-x text-ink-mute";
  chip.textContent = `… ${String(lines)} lines`;
  chip.onclick = onclick;
  return chip;
}

function stepName(tr: Transaction): string {
  let line = 1;
  tr.changes.iterChangedRanges((from) => {
    line = tr.startState.doc.lineAt(from).number;
  }, false);
  const verb = tr.isUserEvent("input.paste")
    ? "pasted"
    : tr.isUserEvent("input")
      ? "typed"
      : tr.isUserEvent("delete")
        ? "deleted"
        : tr.isUserEvent("move")
          ? "moved"
          : "edited";
  return `${verb} on line ${line}`;
}

function foldRanges(state: EditorState): TextRange[] {
  const ranges: TextRange[] = [];
  for (let n = 1; n <= state.doc.lines; n += 1) {
    const line = state.doc.line(n);
    const range = foldable(state, line.from, line.to);
    if (range) ranges.push(range);
  }
  return ranges;
}

function foldWhere(view: EditorView, folds: (depth: number) => boolean) {
  const open: TextRange[] = [];
  const chosen: TextRange[] = [];
  for (const range of foldRanges(view.state)) {
    while (open.length && open[open.length - 1].to < range.to) open.pop();
    if (folds(open.length)) chosen.push(range);
    open.push(range);
  }
  unfoldAll(view);
  const effects = chosen.map((range) => foldEffect.of(range));
  if (effects.length) view.dispatch({ effects });
}

function revealAndScroll(view: EditorView, range: TextRange) {
  const hiding: TextRange[] = [];
  foldedRanges(view.state).between(range.from, range.to, (from, to) => {
    hiding.push({ from, to });
  });
  view.dispatch({
    effects: [
      ...hiding.map((r) => unfoldEffect.of(r)),
      EditorView.scrollIntoView(range.from, { y: "center" }),
    ],
  });
}

/**
 * EditSurface — the data window's whole-data surface: every byte of a JSON, XML or text document, editable, foldable and searchable, uncapped.
 * @tier T2 — owns the editor configuration (editable document, virtualised viewport, fold gutter, three-token highlight, find marks); the text, the one history and what a change means are the caller's
 * @anatomy line numbers (ink-mute) · fold gutter ▾/▸ with a "… N lines" wash-tint chip on a folded line · mono text-body document, syn-key/syn-str/syn-num, punctuation ink-mute, wrap on by default · find marks wash-select, current wash-active + 1px ink-mute outline · CodeMirror cursor and selection · an inset block under one line (the caller's node) · fold to all folds every range inside the outermost, which stays open, so the top level reads as one line each
 * @use the data window only (DEC-75): its json / xml / text surface and a form shown encoded; read-only for a decoded result; the caller names the line break the text is written with, so an edit keeps the ones it did not touch
 * @avoid the body pane or any surface showing captured evidence (UC-5: that is SourceSurface, read-only, and the line between them is DEC-75 rule 5) · keeping its own undo history: every change goes to onChange so the caller's one history holds it, with extend set while CodeMirror's input grouping still holds · reformatting, sorting or decoding inside it — those are the caller's steps, handed back as text · a cap or a cut marker: the data window has none (DEC-75 rule 7) · marking the document edited (DEC-59: the screen says so in words) · overlaying the inset on the lines below it: it is a block in the document · scrolling to a match on its own: the current match is revealed only when the caller bumps reveal (a step, a new search), so an edit that moves the matches leaves the view where the user is
 * @states editable · read-only (selection, find and fold still work) · folded (view state) · with find marks · with an inset under one line. A json document that no longer parses stays json: the parser's recovery keeps highlighting and folding what still reads. No empty, error or loading state
 * @madeof CodeMirror 6 (T0) — view, state, language, lang-json, lang-xml, search decorations
 */
export function EditSurface({
  text,
  language: choice,
  readOnly = false,
  wrap = true,
  matches = NO_MATCHES,
  current = -1,
  reveal = 0,
  onChange,
  onSelect,
  inset = null,
  lineBreak = "\n",
  handle,
  testId,
}: EditSurfaceProps) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const callbacks = useRef({ onChange, onSelect });
  callbacks.current = { onChange, onSelect };
  const group = useRef<{ step: string; at: number } | null>(null);
  const shown = useRef(text);
  const compartments = useRef({ language: new Compartment(), readOnly: new Compartment(), wrap: new Compartment() });
  const joinWith = useRef(lineBreak);
  joinWith.current = lineBreak;
  const [insetHost] = useState(() => document.createElement("div"));

  useImperativeHandle(handle, () => ({
    foldTo(depth) {
      const editor = view.current;
      if (!editor) return;
      if (depth === "none") unfoldAll(editor);
      else if (depth === "all") foldWhere(editor, (at) => at > 0);
      else foldWhere(editor, (at) => at === depth);
    },
    focus() {
      view.current?.focus();
    },
    positionAt(point) {
      return view.current?.posAtCoords(point) ?? null;
    },
    cursor() {
      return view.current?.state.selection.main.head ?? 0;
    },
    selection() {
      const main = view.current?.state.selection.main;
      return { from: main?.from ?? 0, to: main?.to ?? 0 };
    },
    select({ from, to }) {
      view.current?.dispatch({ selection: { anchor: from, head: to }, scrollIntoView: true });
    },
  }));

  useEffect(() => {
    if (!host.current) return;
    const { language: lang, readOnly: locked, wrap: wrapped } = compartments.current;
    const listener = EditorView.updateListener.of((update) => {
      const user = update.transactions.filter((tr) => tr.docChanged && !tr.annotation(external));
      if (user.length) {
        const step = stepName(user[0]);
        const now = Date.now();
        const extend = group.current !== null && group.current.step === step && now - group.current.at < GROUP_MS;
        group.current = { step, at: now };
        shown.current = update.state.doc.sliceString(0, update.state.doc.length, joinWith.current);
        callbacks.current.onChange?.(shown.current, step, extend);
      }
      if (update.selectionSet) {
        const selection = update.state.selection;
        callbacks.current.onSelect?.(
          update.state.doc.sliceString(selection.main.from, selection.main.to, joinWith.current),
          update.state.doc.lineAt(selection.main.to).number,
        );
      }
    });
    const editor = new EditorView({
      state: EditorState.create({
        doc: text,
        extensions: [
          lineNumbers(),
          codeFolding({ preparePlaceholder: (state, range) => state.doc.lineAt(range.to).number - state.doc.lineAt(range.from).number, placeholderDOM: foldPlaceholder }),
          foldGutter({ openText: "▾", closedText: "▸" }),
          syntaxHighlighting(warpHighlight),
          keymap.of([...foldKeymap, ...standardKeymap]),
          warpTheme,
          surfaceTheme,
          lang.of(language(choice)),
          locked.of(EditorState.readOnly.of(readOnly)),
          wrapped.of(wrap ? EditorView.lineWrapping : []),
          marks,
          insetField,
          insetDecorations,
          listener,
        ],
      }),
      parent: host.current,
    });
    view.current = editor;
    return () => {
      editor.destroy();
      view.current = null;
    };
  }, []);

  useEffect(() => {
    const editor = view.current;
    if (!editor || text === shown.current) return;
    shown.current = text;
    group.current = null;
    editor.dispatch({
      changes: { from: 0, to: editor.state.doc.length, insert: text },
      annotations: external.of(true),
    });
  }, [text]);

  useEffect(() => {
    view.current?.dispatch({ effects: compartments.current.language.reconfigure(language(choice)) });
  }, [choice]);

  useEffect(() => {
    view.current?.dispatch({ effects: compartments.current.readOnly.reconfigure(EditorState.readOnly.of(readOnly)) });
  }, [readOnly]);

  useEffect(() => {
    view.current?.dispatch({ effects: compartments.current.wrap.reconfigure(wrap ? EditorView.lineWrapping : []) });
  }, [wrap]);

  useEffect(() => {
    view.current?.dispatch({ effects: setMarks.of({ matches, current }) });
  }, [matches, current, text]);

  const shownMarks = useRef({ matches, current });
  shownMarks.current = { matches, current };
  useEffect(() => {
    const editor = view.current;
    const target = shownMarks.current.matches[shownMarks.current.current];
    if (reveal && editor && target && target.to <= editor.state.doc.length) revealAndScroll(editor, target);
  }, [reveal]);

  const insetLine = inset?.afterLine ?? null;
  useEffect(() => {
    view.current?.dispatch({ effects: setInset.of(insetLine === null ? null : { line: insetLine, host: insetHost }) });
  }, [insetLine, insetHost]);

  return (
    <div
      ref={host}
      data-testid={testId}
      data-wire="working copy"
      className="min-h-0 flex-1 overflow-hidden bg-canvas font-mono text-body leading-normal"
    >
      {inset && createPortal(inset.node, insetHost)}
    </div>
  );
}
