import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent,
  type ReactNode,
  type RefObject,
} from "react";
import { byteLength, captionFilename, decodeBody, hexDump, humanBytes, saveBody } from "./body";
import { claimOnce, openDataWindow, queryCount, takeDataPayload, type DataPayload, type Heading } from "./dataWindow";
import {
  contentKey,
  decode,
  decodeAround,
  DECODER_LABEL,
  hasValueAround,
  opensAs,
  replacement,
  type DecoderId,
  type Decoded,
  type Peeked,
  type Span,
} from "./decoders";
import { decidedBy, readKind } from "./contentKind";
import { DATA_KINDS, READ_ONLY_KINDS, type DataKind } from "./dataKind";
import { findBytes, findMatches, readHexQuery, type HexQueryMode } from "./find";
import {
  addPairAfter,
  editPair,
  findInPairs,
  pairsAsJson,
  parsePairs,
  removePair,
  SORT_BY_KEY_STEP,
  sortByKey,
  type PairMatches,
  type PairPart,
} from "./formPairs";
import {
  indentChosen,
  indentOf,
  INDENT_UNIT,
  INDENTS,
  nodeAt,
  parseJson,
  spliceAsJson,
  pathText,
  reformatJson,
  SORT_STEP,
  surfaceText,
  valueText,
  type Indent,
} from "./jsonTools";
import { loadBeautifier, reformatCode, REFORMAT_STEP, type Beautifier } from "./codeTools";
import { REINDENT_STEP, reindentXml } from "./xmlTools";
import {
  amend,
  canRedo,
  canUndo,
  commit,
  currentStepName,
  editCount,
  isModified,
  lineBreakOf,
  lineCountOf,
  redo,
  restoreOriginal,
  startHistory,
  undo,
  workingBytes,
  workingText,
  type History,
} from "./workingCopy";
import {
  CopyButton,
  EditSurface,
  Field,
  FormTable,
  HexView,
  IconButton,
  ImageRenderer,
  Menu,
  MethodLabel,
  Notice,
  Select,
  useCopy,
  type EditSurfaceHandle,
  type FoldDepth,
  type MenuItem,
  type MenuPoint,
  type TextRange,
} from "./components/index";

type OpenMenu = { type: "more" } | { type: "context"; at: MenuPoint; row: number | null; offset?: number };
type PeekOrigin = { type: "selection"; range: TextRange; line: number } | { type: "row"; index: number; key: string };
type Open = (payload: DataPayload) => Promise<void>;

interface Peek {
  origin: PeekOrigin;
  peeked: Peeked;
}
type NodePart = "path" | "value";
type ImageSize = "fit" | "actual";

const SURFACE_KIND = {
  json: "json",
  form: "form",
  xml: "xml",
  html: "xml",
  javascript: "javascript",
  css: "css",
  text: "text",
  hex: "hex",
  image: "image",
} as const;

const SURFACE_LANGUAGE = { json: "json", xml: "xml", javascript: "javascript", css: "css", text: "plain" } as const;

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
const NO_MATCHES: TextRange[] = [];
const NO_PAIR_MATCHES = new Map<number, PairMatches>();
const FOLD_CHOICES = [
  { id: "1", label: "1" },
  { id: "2", label: "2" },
  { id: "3", label: "3" },
  { id: "all", label: "all" },
  { id: "none", label: "none" },
];
const NEEDS_JSON = "needs valid JSON";
const REPLACED_SELECTION = "replaced the selection with its decoded form";
const SHOWN_ENCODED = "shown encoded";
const REPLACE_LABEL: Record<DecoderId, string> = {
  jwt: "replace with the payload",
  time: "replace with local time",
  base64: "replace with decoded",
  url: "replace with decoded",
};

interface Search {
  ranges: TextRange[];
  byPair: Map<number, PairMatches>;
  total: number;
  capped: boolean;
  as: HexQueryMode | null;
}

const NOTHING_FOUND: Search = { ranges: NO_MATCHES, byPair: NO_PAIR_MATCHES, total: 0, capped: false, as: null };

function findCount(query: string, found: Search, current: number): string {
  if (!query) return "";
  const count = found.total ? `${current + 1} of ${found.total}${found.capped ? "+" : ""}` : "no match";
  return found.as ? `${count} · as ${found.as}` : count;
}

function FindBar({
  query,
  onQuery,
  count,
  total,
  onStep,
  onDismiss,
  box,
}: {
  query: string;
  onQuery: (query: string) => void;
  count: string;
  total: number;
  onStep: (by: number) => void;
  onDismiss: () => void;
  box: RefObject<HTMLDivElement>;
}) {
  const onKeyDown = (event: ReactKeyboardEvent) => {
    if (event.key === "Enter") {
      event.preventDefault();
      onStep(event.shiftKey ? -1 : 1);
    } else if (event.key === "Escape") {
      event.preventDefault();
      onDismiss();
    }
  };
  return (
    <div ref={box} onKeyDown={onKeyDown} className="flex items-center gap-chrome-gap border-b border-hairline px-4 py-chrome-y">
      <Field value={query} onChange={(event) => onQuery(event.target.value)} aria-label="find" placeholder="find" verbatim className="min-w-0 flex-1" />
      <span data-testid="find-count" className="whitespace-nowrap font-ui text-chrome text-ink-mute">
        {count}
      </span>
      <IconButton glyph="previous" label="previous match" onClick={() => onStep(-1)} disabled={total === 0} />
      <IconButton glyph="next" label="next match" onClick={() => onStep(1)} disabled={total === 0} />
    </div>
  );
}

function DecodePeek({
  peeked,
  from,
  onReplace,
  onOpen,
  onClose,
}: {
  peeked: Peeked;
  from: "selection" | "value";
  onReplace: (() => void) | null;
  onOpen: (decoder: DecoderId, reading: Decoded) => void;
  onClose: () => void;
}) {
  const block = (label: string, child: ReactNode) => (
    <div className="flex flex-col gap-chrome-tight">
      <span className="font-ui text-label uppercase tracking-label text-ink-mute">{label}</span>
      {child}
    </div>
  );
  const shown = (text: string, language: "json" | "plain") => (
    <EditSurface text={text} language={language} readOnly testId="decoded" />
  );
  const body = () => {
    if (peeked.decoder === null) {
      return (
        <Notice tone="warn" detail={`the ${from} is left as it was`}>
          {`not base64, URL-encoded, a JWT or a time: ${peeked.reason}`}
        </Notice>
      );
    }
    const { reading } = peeked;
    if (reading.kind === "jwt") {
      return (
        <div className="flex flex-col gap-2">
          {block("header", shown(reading.header, "json"))}
          {block("payload", shown(reading.payload, "json"))}
          {block(
            "signature · not verified",
            <span data-testid="signature" className="break-all font-mono text-body text-ink-body">
              {reading.signature || "none"}
            </span>,
          )}
        </div>
      );
    }
    if (reading.kind === "time") {
      const line = (text: string, testId: string) => (
        <span data-testid={testId} className="break-all font-mono text-body text-ink-body">
          {text}
        </span>
      );
      return (
        <div className="flex flex-col gap-2">
          {reading.utc === null ? (
            block("date", line(reading.local, "local-time"))
          ) : (
            <>
              {block(`local · ${reading.zone}`, line(reading.local, "local-time"))}
              {block("UTC", line(reading.utc, "utc-time"))}
            </>
          )}
          {block("read as", line(`${reading.readAs} · ${reading.relative}`, "read-as"))}
        </div>
      );
    }
    if (reading.kind === "binary") {
      return (
        <Notice detail="binary cannot replace text in place; ↗ opens it as data of its own">
          {`${humanBytes(reading.bytes.length)} of binary · ${reading.looks ? `looks like ${reading.looks}` : "no known signature"}`}
        </Notice>
      );
    }
    return reading.kind === "json" ? shown(reading.shown, "json") : shown(reading.text, "plain");
  };
  return (
    <div data-testid="decode-peek" className="rounded-card border border-hairline bg-canvas-soft font-ui">
      <div className="flex flex-wrap items-center gap-chrome-gap border-b border-hairline px-3 py-chrome-y">
        <span className="text-label uppercase tracking-label text-ink-mute">{peeked.decoder ? "decoded as" : "decode"}</span>
        {peeked.decoder && <span className="text-control text-ink">{DECODER_LABEL[peeked.decoder]}</span>}
        <span className="ml-auto flex items-center gap-chrome-gap">
          {onReplace && peeked.decoder && <IconButton glyph="replace" label={REPLACE_LABEL[peeked.decoder]} onClick={onReplace} />}
          {peeked.decoder && (
            <IconButton glyph="window" label="open in data window" onClick={() => onOpen(peeked.decoder, peeked.reading)} />
          )}
          <IconButton glyph="close" label="close the peek" onClick={onClose} />
        </span>
      </div>
      <div className="max-h-body-max overflow-auto px-3 py-2">{body()}</div>
    </div>
  );
}

function Caption({ title, heading }: { title: string; heading?: Heading }) {
  if (!heading) {
    return (
      <h1 title={title} className="min-w-0 flex-1 truncate font-ui text-note text-ink">
        {title}
      </h1>
    );
  }
  const params = queryCount(heading.query);
  return (
    <div className="min-w-0 flex-1">
      <p className="font-ui text-chrome uppercase tracking-label text-ink-mute">{heading.what}</p>
      <h1 aria-label={title} title={title} className="flex min-w-0 items-baseline gap-2">
        <MethodLabel method={heading.method} className="shrink-0" />
        <span className="min-w-0 truncate font-mono text-body">
          <span className="text-ink">{heading.host}</span>
          <span className="text-ink-body">{heading.path}</span>
          <span className="text-ink-mute">{heading.query}</span>
        </span>
        {params > 0 && <span className="shrink-0 font-ui text-chrome text-ink-mute">{plural(params, "param")}</span>}
      </h1>
    </div>
  );
}

const SETTLE_MS = 250;

function useSettled<T>(value: T): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), SETTLE_MS);
    return () => clearTimeout(timer);
  }, [value]);
  return settled;
}

function Workbench({ payload, open }: { payload: DataPayload; open: Open }) {
  const reading = useMemo(() => readKind(payload.bytes, payload.typeHint), [payload]);
  const decoded = useMemo(() => decodeBody(payload.bytes, { keepBom: true }), [payload]);
  const original = decoded.text;
  const [history, setHistory] = useState<History>(() => startHistory(original));
  const [override, setOverride] = useState<DataKind | null>(null);
  const [menu, setMenu] = useState<OpenMenu | null>(null);
  const [selection, setSelection] = useState("");
  const [row, setRow] = useState<number | null>(null);
  const [size, setSize] = useState<ImageSize>("fit");
  const [query, setQuery] = useState("");
  const [at, setAt] = useState(0);
  const [reveal, setReveal] = useState(0);
  const [fold, setFold] = useState<{ id: string; text: string } | null>(null);
  const [wrap, setWrap] = useState(true);
  const [encoded, setEncoded] = useState(false);
  const [peek, setPeek] = useState<Peek | null>(null);
  const more = useRef<HTMLButtonElement>(null);
  const surfaceHandle = useRef<EditSurfaceHandle>(null);
  const findBox = useRef<HTMLDivElement>(null);
  const { state: copied, copy } = useCopy(payload);
  const settled = useSettled(history);

  const picked = override ?? reading.kind;
  const kind = SURFACE_KIND[picked];
  const lineBreak = useMemo(() => lineBreakOf(original), [original]);
  const lossy = !decoded.whole && !READ_ONLY_KINDS.has(kind);
  const mixed = lineBreak === null && !READ_ONLY_KINDS.has(kind);
  const readOnly = READ_ONLY_KINDS.has(kind) || lossy || mixed;
  const text = workingText(history);
  const shownBytes = useMemo(
    () => (READ_ONLY_KINDS.has(kind) ? workingBytes(history, payload.bytes) : null),
    [kind, history, payload.bytes],
  );
  const shownSize = useMemo(
    () => (isModified(settled) ? byteLength(workingText(settled)) : payload.bytes.length),
    [settled, payload.bytes],
  );
  const pairs = useMemo(() => (kind === "form" ? parsePairs(text) : []), [kind, text]);
  const lines = useMemo(() => lineCountOf(workingText(settled)), [settled]);
  const empty = payload.bytes.length === 0;
  const editableForm = kind === "form" && !lossy && !mixed;
  const asTable = editableForm && !encoded;
  const onText = !empty && kind !== "image" && kind !== "hex" && !asTable;
  const searchable = !empty && kind !== "image";
  const breakWith = lineBreak ?? "\n";
  const found = useMemo((): Search => {
    if (!query) return NOTHING_FOUND;
    if (onText) {
      const { ranges, capped } = findMatches(surfaceText(text), query);
      return { ...NOTHING_FOUND, ranges, total: ranges.length, capped };
    }
    if (asTable) {
      const { byPair, total: count, capped } = findInPairs(pairs, query);
      return { ...NOTHING_FOUND, byPair, total: count, capped };
    }
    if (kind === "hex" && shownBytes) {
      const hexQuery = readHexQuery(query);
      const { ranges, capped } = findBytes(shownBytes, hexQuery);
      return { ...NOTHING_FOUND, ranges, total: ranges.length, capped, as: hexQuery.mode };
    }
    return NOTHING_FOUND;
  }, [asTable, kind, onText, pairs, query, shownBytes, text]);
  const total = found.total;
  const current = total ? at % total : -1;
  const settledJson = useMemo(
    () => (kind === "json" && !empty ? parseJson(workingText(settled)) : null),
    [kind, empty, settled],
  );
  const menuJson = useMemo(() => (kind === "json" && menu ? parseJson(surfaceText(text)) : null), [kind, menu, text]);
  const parses = menuJson?.ok ?? false;
  const codeKind = kind === "javascript" || kind === "css" ? kind : null;
  const [beautifier, setBeautifier] = useState<Beautifier | null>(null);
  useEffect(() => {
    if (!codeKind) return;
    let live = true;
    void loadBeautifier().then((loaded) => live && setBeautifier(loaded));
    return () => {
      live = false;
    };
  }, [codeKind]);
  const menuCode = useMemo(
    () => (codeKind && !readOnly && menu && beautifier ? reformatCode(beautifier, codeKind, text, breakWith) : null),
    [codeKind, readOnly, menu, beautifier, text, breakWith],
  );
  const menuXml = useMemo(
    () => (kind === "xml" && !readOnly && menu ? reindentXml(text, breakWith) : null),
    [kind, readOnly, menu, text, breakWith],
  );

  useEffect(() => setPeek(null), [text, kind, encoded]);

  const step = (next: string, name: string) => setHistory((h) => commit(h, next, name));
  const stepFind = useCallback(
    (by: number) => {
      if (!total) return;
      setAt((current + by + total) % total);
      setReveal((n) => n + 1);
    },
    [current, total],
  );
  const search = (next: string) => {
    setQuery(next);
    setAt(0);
    setReveal((n) => n + 1);
  };
  const dismissFind = () => {
    search("");
    if (surfaceHandle.current) surfaceHandle.current.focus();
    else findBox.current?.querySelector("input")?.blur();
  };
  const reformat = (indent: Indent) => {
    const name = INDENTS.find((choice) => choice.id === indent)?.step ?? indent;
    setHistory((h) => {
      const result = reformatJson(workingText(h), INDENT_UNIT[indent], false, breakWith);
      return result.ok ? commit(h, result.text, name) : h;
    });
  };
  const sortKeys = () =>
    setHistory((h) => {
      const working = workingText(h);
      const result = reformatJson(working, indentOf(working), true, breakWith);
      return result.ok ? commit(h, result.text, SORT_STEP) : h;
    });
  const sortPairs = () => setHistory((h) => commit(h, sortByKey(workingText(h)), SORT_BY_KEY_STEP));
  const reindent = useCallback(
    () =>
      setHistory((h) => {
        const result = reindentXml(workingText(h), breakWith);
        return result.ok ? commit(h, result.text, REINDENT_STEP) : h;
      }),
    [breakWith],
  );
  const reformatSource = useCallback(
    () =>
      setHistory((h) => {
        if (!codeKind || !beautifier) return h;
        const result = reformatCode(beautifier, codeKind, workingText(h), breakWith);
        return result.ok ? commit(h, result.text, REFORMAT_STEP) : h;
      }),
    [beautifier, breakWith, codeKind],
  );
  const toggleEncoded = () => {
    setEncoded((shown) => !shown);
    setRow(null);
  };
  const foldTo = (id: string) => {
    surfaceHandle.current?.foldTo(/^\d+$/.test(id) ? Number(id) : (id as FoldDepth));
    setFold({ id, text });
  };
  const copyNode = useCallback(
    (offset: number, part: NodePart) => {
      const source = surfaceText(text);
      const parsed = parseJson(source);
      if (!parsed.ok) return;
      const node = nodeAt(parsed.root, offset);
      copy(part === "path" ? pathText(node.path) : valueText(source, node.node, breakWith));
    },
    [breakWith, copy, text],
  );
  const currentBytes = () => shownBytes ?? workingBytes(history, payload.bytes);
  const onScreen = () => (shownBytes && kind === "hex" ? hexDump(shownBytes, shownBytes.length) : text);
  const save = useCallback(() => {
    void saveBody(shownBytes ?? workingBytes(history, payload.bytes), captionFilename(payload.title, payload.typeHint));
  }, [shownBytes, history, payload]);
  const saveOriginal = () => {
    void saveBody(payload.bytes, captionFilename(payload.title, payload.typeHint, "-original"));
  };

  const decodeAt = useCallback(
    (around: Span) => {
      const surface = surfaceText(text);
      const found = decodeAround(surface, around, { json: kind === "json" });
      if (!found) return;
      surfaceHandle.current?.select(found.range);
      const line = (surface.slice(0, found.range.to).match(/\n/g)?.length ?? 0) + 1;
      setPeek({ origin: { type: "selection", range: found.range, line }, peeked: found.peeked });
    },
    [kind, text],
  );
  const decodeRow = useCallback(
    (index: number) => {
      const pair = pairs[index];
      if (pair?.value) setPeek({ origin: { type: "row", index, key: pair.key }, peeked: decode(pair.value) });
    },
    [pairs],
  );
  const decodeNow = useCallback(() => {
    if (asTable && row !== null) decodeRow(row);
    else if (onText && !readOnly && surfaceHandle.current) decodeAt(surfaceHandle.current.selection());
  }, [asTable, decodeAt, decodeRow, onText, readOnly, row]);
  const replaceDecoded = (origin: PeekOrigin, value: string) => {
    if (origin.type === "row") {
      step(editPair(text, origin.index, "value", value), `replaced the value of “${origin.key}” with its decoded form`);
      return;
    }
    const surface = surfaceText(text);
    const { from, to } = origin.range;
    const spliced =
      kind === "json" ? spliceAsJson(surface, origin.range, value) : surface.slice(0, from) + surfaceText(value) + surface.slice(to);
    step(spliced.replace(/\n/g, breakWith), REPLACED_SELECTION);
  };
  const openDecoded = (decoder: DecoderId, reading: Decoded) => {
    const { bytes, typeHint } = opensAs(reading);
    const title = `${payload.title} · decoded ${DECODER_LABEL[decoder]}`;
    void contentKey(bytes).then((key) => open(typeHint ? { key, title, bytes, typeHint } : { key, title, bytes }));
  };
  const peekNode = () => {
    if (!peek) return null;
    const value = peek.peeked.decoder ? replacement(peek.peeked.reading) : null;
    return (
      <DecodePeek
        peeked={peek.peeked}
        from={peek.origin.type === "row" ? "value" : "selection"}
        onReplace={value === null || readOnly ? null : () => replaceDecoded(peek.origin, value)}
        onOpen={openDecoded}
        onClose={() => setPeek(null)}
      />
    );
  };

  const removeRow = useCallback(
    (index: number) => {
      setHistory((h) => commit(h, removePair(workingText(h), index), `removed pair ${index + 1}`));
      setRow(null);
    },
    [],
  );

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const command = event.metaKey || event.ctrlKey;
      const typing = event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement;
      if (empty) return;
      if (event.key === "Escape" && peek && !menu && !typing) {
        event.preventDefault();
        setPeek(null);
        return;
      }
      if (!command && event.altKey && !typing) {
        if (event.code === "KeyZ" && !event.shiftKey && onText) {
          event.preventDefault();
          setWrap((on) => !on);
        } else if (event.code === "KeyF" && event.shiftKey && kind === "xml" && !readOnly) {
          event.preventDefault();
          reindent();
        } else if (event.code === "KeyF" && event.shiftKey && codeKind && !readOnly) {
          event.preventDefault();
          reformatSource();
        }
        return;
      }
      if (!command) return;
      const key = event.key.toLowerCase();
      if (key === "d" && event.shiftKey && !typing) {
        event.preventDefault();
        decodeNow();
      } else if (key === "f" && searchable) {
        event.preventDefault();
        const input = findBox.current?.querySelector("input");
        input?.focus();
        input?.select();
      } else if (key === "g" && searchable) {
        event.preventDefault();
        stepFind(event.shiftKey ? -1 : 1);
      } else if (event.code === "KeyC" && event.altKey && kind === "json" && !typing && surfaceHandle.current) {
        event.preventDefault();
        copyNode(surfaceHandle.current.cursor(), event.shiftKey ? "value" : "path");
      } else if (key === "s") {
        event.preventDefault();
        save();
      } else if (key === "z" && !typing && !readOnly) {
        event.preventDefault();
        setHistory((h) => (event.shiftKey ? redo(h) : undo(h)));
      } else if (event.key === "Backspace" && !typing && asTable && row !== null) {
        event.preventDefault();
        removeRow(row);
      } else if (key === "c" && !typing && asTable && row !== null && pairs[row] && !window.getSelection()?.toString()) {
        event.preventDefault();
        copy(pairs[row].value);
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [asTable, codeKind, copy, copyNode, decodeNow, empty, kind, menu, onText, pairs, peek, readOnly, reformatSource, reindent, removeRow, row, save, searchable, stepFind]);

  const moreSections = (): MenuItem[][] => {
    const sections: MenuItem[][] = [];
    if (!readOnly) {
      sections.push([
        { label: "undo", keys: "⌘Z", disabled: !canUndo(history), why: "nothing to undo", run: () => setHistory(undo) },
        { label: "redo", keys: "⇧⌘Z", disabled: !canRedo(history), why: "nothing to redo", run: () => setHistory(redo) },
        {
          label: "restore original",
          disabled: !isModified(history),
          why: "unmodified",
          run: () => setHistory(restoreOriginal),
        },
      ]);
    }
    const foldRow: MenuItem = {
      label: "fold to",
      choices: FOLD_CHOICES,
      value: fold?.text === text ? fold.id : null,
      pick: foldTo,
    };
    const wrapRow: MenuItem = { label: "wrap long lines", keys: "⌥Z", checked: wrap, run: () => setWrap((on) => !on) };
    if (kind === "json") {
      sections.push([
        ...(readOnly
          ? []
          : [
              {
                label: "indent",
                choices: INDENTS.map(({ id, label }) => ({ id, label })),
                value: parses ? indentChosen(currentStepName(history)) : null,
                disabled: !parses,
                why: NEEDS_JSON,
                pick: (id: string) => reformat(id as Indent),
              },
              { label: "sort keys", disabled: !parses, why: NEEDS_JSON, run: sortKeys },
            ]),
        foldRow,
        wrapRow,
      ]);
    }
    if (kind === "xml") {
      sections.push([
        ...(menuXml
          ? [
              {
                label: "re-indent",
                keys: "⌥⇧F",
                disabled: !menuXml.ok,
                why: menuXml.ok ? undefined : `not well-formed at line ${menuXml.line}, column ${menuXml.column}: ${menuXml.reason}`,
                run: reindent,
              },
            ]
          : []),
        foldRow,
        wrapRow,
      ]);
    }
    if (codeKind) {
      sections.push([
        ...(readOnly
          ? []
          : [
              {
                label: "reformat",
                keys: "⌥⇧F",
                disabled: !menuCode?.ok,
                why: menuCode ? (menuCode.ok ? undefined : menuCode.reason) : "the formatter is loading",
                run: reformatSource,
              },
            ]),
        foldRow,
        wrapRow,
      ]);
    }
    if (kind === "text" || (kind === "form" && onText)) sections.push([wrapRow]);
    if (editableForm) {
      sections.push([
        { label: "show encoded", checked: encoded, run: toggleEncoded },
        { label: "sort by key", disabled: encoded, why: SHOWN_ENCODED, run: sortPairs },
        { label: "copy as JSON", run: () => copy(pairsAsJson(pairs)) },
      ]);
    }
    if (kind === "image") {
      sections.push([
        {
          label: "size",
          choices: [
            { id: "fit", label: "fit" },
            { id: "actual", label: "actual" },
          ],
          value: size,
          pick: (id) => setSize(id as ImageSize),
        },
      ]);
    }
    sections.push(
      readOnly
        ? [{ label: "save", keys: "⌘S", run: save }]
        : [
            { label: "copy original", run: () => copy(original) },
            { label: "save", keys: "⌘S", run: save },
            { label: "save original", run: saveOriginal },
          ],
    );
    return sections;
  };

  const contextSections = (target: number | null, offset: number | undefined): MenuItem[][] => {
    const pair = target === null ? undefined : pairs[target];
    if (asTable && pair) {
      return [
        [
          { label: "decode the value", keys: "⌘⇧D", disabled: !pair.value, why: "empty value", run: () => decodeRow(pair.index) },
          { label: "copy the value", keys: "⌘C", run: () => copy(pair.value) },
        ],
        [
          { label: "add a pair below", run: () => step(addPairAfter(text, pair.index), `added a pair after pair ${pair.index + 1}`) },
          { label: "remove the pair", keys: "⌘⌫", run: () => removeRow(pair.index) },
        ],
      ];
    }
    const selected = surfaceHandle.current?.selection();
    const around = selection && selected ? selected : { from: offset ?? 0, to: offset ?? 0 };
    const sections: MenuItem[][] = [
      [
        {
          label: "decode",
          keys: "⌘⇧D",
          disabled: !hasValueAround(surfaceText(text), around),
          why: "no value here: right-click on one or select it",
          run: () => decodeAt(around),
        },
        { label: "copy", keys: "⌘C", disabled: !selection, why: "select some text", run: () => copy(selection) },
      ],
    ];
    if (kind === "json" && offset !== undefined) {
      sections.push([
        { label: "copy the path", keys: "⌥⌘C", disabled: !parses, why: NEEDS_JSON, run: () => copyNode(offset, "path") },
        { label: "copy the value", keys: "⇧⌥⌘C", disabled: !parses, why: NEEDS_JSON, run: () => copyNode(offset, "value") },
      ]);
    }
    return sections;
  };

  const onContext = (event: MouseEvent) => {
    if (readOnly || asTable) return;
    event.preventDefault();
    const point = { x: event.clientX, y: event.clientY };
    const handle = surfaceHandle.current;
    const offset = handle ? (handle.positionAt(point) ?? handle.cursor()) : undefined;
    setMenu({ type: "context", at: point, row: null, offset });
  };

  const surface = () => {
    if (empty) {
      return (
        <div className="flex flex-1 items-center justify-center">
          <Notice layout="region" detail="the data handed in is 0 B: there is nothing to show, edit or decode">
            none
          </Notice>
        </div>
      );
    }
    if (kind === "image") {
      return (
        <div className="min-h-0 flex-1 overflow-auto p-4">
          <div className={size === "fit" ? "" : "w-max"}>
            <ImageRenderer bytes={currentBytes()} contentType={payload.typeHint ?? null} />
          </div>
        </div>
      );
    }
    if (kind === "hex") {
      return <HexView bytes={currentBytes()} matches={found.ranges} current={current} reveal={reveal} onCopy={copy} testId="data-surface" />;
    }
    if (asTable) {
      const edit = (index: number, part: PairPart, value: string) =>
        step(editPair(text, index, part, value), `edited the ${part} of pair ${index + 1}`);
      return (
        <FormTable
          pairs={pairs}
          selected={row}
          onSelect={setRow}
          onEdit={edit}
          onAdd={(after) => step(addPairAfter(text, after), after < 0 ? "added a pair" : `added a pair after pair ${after + 1}`)}
          onRemove={removeRow}
          onContext={(index, at) => setMenu({ type: "context", at, row: index })}
          matches={found.byPair}
          current={current}
          reveal={reveal}
          inset={peek?.origin.type === "row" ? { afterIndex: peek.origin.index, node: peekNode() } : null}
          testId="data-surface"
        />
      );
    }
    return (
      <div className="flex min-h-0 flex-1 flex-col" onContextMenu={onContext}>
        {mixed && !lossy && (
          <div className="border-b border-hairline px-4 py-chrome-y">
            <Notice tone="warn" detail="an edit would rewrite every break to one kind, so it stays read-only; hex shows every byte">
              mixed line breaks: CRLF, CR and LF in one text
            </Notice>
          </div>
        )}
        {settledJson && !settledJson.ok && (
          <div className="border-b border-hairline px-4 py-chrome-y">
            <Notice tone="warn" detail="find and fold still work; indent and sort keys wait for it to parse">
              {`not valid JSON at line ${settledJson.line}, column ${settledJson.column}: ${settledJson.reason}`}
            </Notice>
          </div>
        )}
        {lossy && (
          <div className="border-b border-hairline px-4 py-chrome-y">
            <Notice tone="warn" detail="an edit would rewrite those bytes too, so it stays read-only; hex shows every byte">
              not valid UTF-8: shown with � where bytes did not decode
            </Notice>
          </div>
        )}
        <EditSurface
          text={text}
          language={kind === "form" ? "plain" : SURFACE_LANGUAGE[kind]}
          readOnly={lossy || mixed}
          wrap={wrap}
          lineBreak={breakWith}
          onChange={(next, name, extend) => setHistory((h) => (extend ? amend(h, next, name) : commit(h, next, name)))}
          onSelect={(selected) => setSelection(selected)}
          matches={found.ranges}
          current={current}
          reveal={reveal}
          inset={peek?.origin.type === "selection" ? { afterLine: peek.origin.line, node: peekNode() } : null}
          handle={surfaceHandle}
          testId="data-surface"
        />
      </div>
    );
  };

  return (
    <div className="flex h-full flex-col overflow-hidden bg-canvas text-ink">
      <header className="flex items-center gap-chrome-gap border-b border-hairline px-4 py-2">
        <Caption title={payload.title} heading={payload.heading} />
        {!empty && kind !== "image" && <CopyButton label="copy" getPayload={onScreen} />}
        {!empty && (
          <span ref={more}>
            <IconButton
              glyph="more"
              label="more"
              expanded={menu?.type === "more"}
              onClick={() => setMenu((open) => (open?.type === "more" ? null : { type: "more" }))}
            />
          </span>
        )}
      </header>

      {searchable && (
        <FindBar
          query={query}
          onQuery={search}
          count={findCount(query, found, current)}
          total={total}
          onStep={stepFind}
          onDismiss={dismissFind}
          box={findBox}
        />
      )}

      {surface()}

      <footer className="flex flex-wrap items-center gap-x-3 gap-y-chrome-tight border-t border-hairline px-4 py-chrome-y font-ui text-chrome text-ink-mute">
        <Select
          options={DATA_KINDS}
          value={picked}
          onChange={(id) => {
            setOverride(id === reading.kind ? null : (id as DataKind));
            setMenu(null);
          }}
          label="treat the data as"
          searchable={false}
          opens="above"
        />
        <span data-testid="detected">
          {decidedBy(reading, override !== null)}
        </span>
        <span>{humanBytes(shownSize)}</span>
        {!readOnly && !empty && <span>{asTable ? plural(pairs.length, "pair") : plural(lines, "line")}</span>}
        <span className="ml-auto flex items-center gap-x-3">
          {copied === "confirming" && <span className="text-ok">copied</span>}
          {copied === "error" && <span className="text-warn">copy failed</span>}
          {selection && !asTable && !readOnly && <span>selection {plural(selection.length, "character")}</span>}
          {readOnly ? (
            <span>read-only</span>
          ) : (
            isModified(history) && (
              <span data-testid="modified" className="text-ink">
                modified · {plural(editCount(history), "edit")}
              </span>
            )
          )}
        </span>
      </footer>

      {menu?.type === "more" && (
        <Menu label="more" sections={moreSections()} anchor={more} onClose={() => setMenu(null)} />
      )}
      {menu?.type === "context" && (
        <Menu label="the selection" sections={contextSections(menu.row, menu.offset)} at={menu.at} onClose={() => setMenu(null)} />
      )}
    </div>
  );
}

export function DataWindowScreen({
  take = takeDataPayload,
  open = openDataWindow,
}: {
  take?: () => Promise<DataPayload | null>;
  open?: Open;
}) {
  const [payload, setPayload] = useState<DataPayload | null | undefined>(undefined);

  useEffect(() => {
    let current = true;
    claimOnce(take).then(
      (claimed) => {
        if (current) setPayload(claimed);
      },
      () => {
        if (current) setPayload(null);
      },
    );
    return () => {
      current = false;
    };
  }, [take]);

  useEffect(() => {
    if (payload) document.title = payload.title;
  }, [payload]);

  if (payload === undefined) {
    return (
      <div className="flex h-full items-center justify-center bg-canvas">
        <Notice layout="region" tone="pending">
          loading…
        </Notice>
      </div>
    );
  }
  if (payload === null) {
    return (
      <div className="flex h-full flex-col bg-canvas text-ink">
        <p className="border-b border-hairline px-4 py-2 font-ui text-note text-ink-mute">data window</p>
        <div className="flex flex-1 items-center justify-center">
          <Notice
            layout="region"
            detail="A data window holds only what its opener hands it, and this one was opened without data. Open it again from a body."
          >
            nothing was handed to this window
          </Notice>
        </div>
      </div>
    );
  }
  return <Workbench payload={payload} open={open} />;
}
