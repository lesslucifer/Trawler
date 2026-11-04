import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { byteLength, captionFilename, hexDump, humanBytes, saveBody } from "./body";
import { claimOnce, takeDataPayload, type DataPayload } from "./dataWindow";
import { DATA_KINDS, detectKind, kindLabel, READ_ONLY_KINDS, type DataKind } from "./dataKind";
import { addPairAfter, editPair, parsePairs, removePair, type PairPart } from "./formPairs";
import {
  amend,
  canRedo,
  canUndo,
  commit,
  decodeOriginal,
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
  FormTable,
  HexView,
  IconButton,
  ImageRenderer,
  Menu,
  Notice,
  Select,
  useConfirm,
  writeClipboard,
  type MenuItem,
  type MenuPoint,
} from "./components/index";

type OpenMenu = { type: "more" } | { type: "context"; at: MenuPoint; row: number | null };
type ImageSize = "fit" | "actual";

const SURFACE_LANGUAGE = { json: "json", xml: "xml", text: "plain" } as const;

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;


function Workbench({ payload }: { payload: DataPayload }) {
  const detected = useMemo(() => detectKind(payload.typeHint, payload.bytes), [payload]);
  const decoded = useMemo(() => decodeOriginal(payload.bytes), [payload]);
  const original = decoded.text;
  const [history, setHistory] = useState<History>(() => startHistory(original));
  const [override, setOverride] = useState<DataKind | null>(null);
  const [menu, setMenu] = useState<OpenMenu | null>(null);
  const [selection, setSelection] = useState("");
  const [row, setRow] = useState<number | null>(null);
  const [size, setSize] = useState<ImageSize>("fit");
  const more = useRef<HTMLButtonElement>(null);
  const { state: copied, flash } = useConfirm(payload);
  const copy = useCallback(
    (text: string) => {
      writeClipboard(text).then(
        () => flash("confirming"),
        () => flash("error"),
      );
    },
    [flash],
  );

  const kind = override ?? detected.kind;
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
    () => (isModified(history) ? byteLength(workingText(history)) : payload.bytes.length),
    [history, payload.bytes],
  );
  const pairs = useMemo(() => (kind === "form" ? parsePairs(text) : []), [kind, text]);
  const lines = useMemo(() => lineCountOf(text), [text]);
  const empty = payload.bytes.length === 0;

  const step = (next: string, name: string) => setHistory((h) => commit(h, next, name));
  const currentBytes = () => shownBytes ?? workingBytes(history, payload.bytes);
  const onScreen = () => (shownBytes && kind === "hex" ? hexDump(shownBytes, shownBytes.length) : text);
  const save = useCallback(() => {
    void saveBody(shownBytes ?? workingBytes(history, payload.bytes), captionFilename(payload.title, payload.typeHint));
  }, [shownBytes, history, payload]);
  const saveOriginal = () => {
    void saveBody(payload.bytes, captionFilename(payload.title, payload.typeHint, "-original"));
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
      if (!command || empty) return;
      const key = event.key.toLowerCase();
      if (key === "s") {
        event.preventDefault();
        save();
      } else if (key === "z" && !typing && !readOnly) {
        event.preventDefault();
        setHistory((h) => (event.shiftKey ? redo(h) : undo(h)));
      } else if (event.key === "Backspace" && !typing && !readOnly && kind === "form" && row !== null) {
        event.preventDefault();
        removeRow(row);
      } else if (key === "c" && !typing && kind === "form" && row !== null && pairs[row] && !window.getSelection()?.toString()) {
        event.preventDefault();
        copy(pairs[row].value);
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [copy, empty, kind, pairs, readOnly, removeRow, row, save]);

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

  const contextSections = (target: number | null): MenuItem[][] => {
    const pair = target === null ? undefined : pairs[target];
    if (kind === "form" && pair) {
      return [
        [{ label: "copy the value", keys: "⌘C", run: () => copy(pair.value) }],
        [
          { label: "add a pair below", run: () => step(addPairAfter(text, pair.index), `added a pair after pair ${pair.index + 1}`) },
          { label: "remove the pair", keys: "⌘⌫", run: () => removeRow(pair.index) },
        ],
      ];
    }
    return [[{ label: "copy", keys: "⌘C", disabled: !selection, why: "select some text", run: () => copy(selection) }]];
  };

  const onContext = (event: MouseEvent) => {
    if (readOnly || kind === "form") return;
    event.preventDefault();
    setMenu({ type: "context", at: { x: event.clientX, y: event.clientY }, row: null });
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
    if (kind === "hex") return <HexView bytes={currentBytes()} onCopy={copy} testId="data-surface" />;
    if (kind === "form" && !lossy && !mixed) {
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
          lineBreak={lineBreak ?? "\n"}
          onChange={(next, name, extend) => setHistory((h) => (extend ? amend(h, next, name) : commit(h, next, name)))}
          onSelect={(selected) => setSelection(selected)}
          testId="data-surface"
        />
      </div>
    );
  };

  return (
    <div className="flex h-full flex-col bg-canvas text-ink">
      <header className="flex items-center gap-chrome-gap border-b border-hairline px-4 py-2">
        <h1 className="min-w-0 flex-1 break-words font-ui text-note text-ink">{payload.title}</h1>
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

      {surface()}

      <footer className="flex flex-wrap items-center gap-x-3 gap-y-chrome-tight border-t border-hairline px-4 py-chrome-y font-ui text-chrome text-ink-mute">
        <Select
          options={DATA_KINDS}
          value={kind}
          onChange={(id) => {
            setOverride(id === detected.kind ? null : (id as DataKind));
            setMenu(null);
          }}
          label="treat the data as"
          searchable={false}
        />
        <span data-testid="detected">
          {override ? `your choice · detected ${kindLabel(detected.kind)} from ${detected.from}` : `detected from ${detected.from}`}
        </span>
        <span>{humanBytes(shownSize)}</span>
        {!readOnly && !empty && <span>{kind === "form" ? plural(pairs.length, "pair") : plural(lines, "line")}</span>}
        <span className="ml-auto flex items-center gap-x-3">
          {copied === "confirming" && <span className="text-ok">copied</span>}
          {copied === "error" && <span className="text-warn">copy failed</span>}
          {selection && kind !== "form" && !readOnly && <span>selection {plural(selection.length, "character")}</span>}
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
        <Menu label="the selection" sections={contextSections(menu.row)} at={menu.at} onClose={() => setMenu(null)} />
      )}
    </div>
  );
}

export function DataWindowScreen({ take = takeDataPayload }: { take?: () => Promise<DataPayload | null> }) {
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
  return <Workbench payload={payload} />;
}
