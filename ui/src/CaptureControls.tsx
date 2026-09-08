import { useEffect, useRef, useState } from "react";
import type { Filter, TrawlerApi } from "./api";
import { openFile, saveBody, type ChosenFile } from "./body";
import { CaptureMenu, type CaptureAct } from "./CaptureMenu";
import { ModalShell } from "./ModalShell";
import { Button, Notice, Spinner, TonedText } from "./components/index";

export const CAPTURE_COPY = {
  trigger: "capture actions",
  exportAct: "export as har…",
  importAct: "import…",
  importName: "import a curl command or a har file",
  clearAct: "clear…",
  exporting: "exporting…",
  importing: "importing…",
  clearConsequence: "the capture is not saved anywhere",
  cancel: "cancel",
  clear: "clear",
  choose: "choose a file…",
  import: "import",
  pasteLabel: "a curl command",
  pastePlaceholder: "paste a curl command",
} as const;

// Each act wears the face the app already spends on its direction: ↓ is
// IconButton's save, ↑ is the mark FlowList prints on an imported row, and ⌫ is
// the only one minted here — the two directions move data, the third erases it,
// which is the boundary the menu's own hairline already draws.
export const CAPTURE_GLYPH = {
  export: "↓",
  import: "↑",
  clear: "⌫",
} as const;

const flowCount = (n: number) => `${n} ${n === 1 ? "flow" : "flows"}`;

// A′ — the header's trailing cluster: one fixed-width slot and one trigger.
// The slot's width is fixed on purpose. It carries the count at rest and the
// in-flight line during a transfer, and if it resized between them, starting an
// export would re-allocate the header's slack and move the filter field the user
// may be mid-keystroke in.
export function CaptureControls({
  api,
  filter,
  count,
  onCleared,
}: {
  api: TrawlerApi;
  filter: Filter;
  count: number;
  onCleared: () => void;
}) {
  // Two flags rather than one slot naming the running transfer: export and
  // import start and finish independently, so a single slot lets whichever
  // finishes first speak for the other — clearing a line that is still true and
  // re-offering an act that is still running.
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  const [asking, setAsking] = useState<"clear" | "import" | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  const exportHar = async () => {
    setFailure(null);
    setExporting(true);
    try {
      const { bytes, filename } = await api.exportHar(filter);
      // saveBody answers null when the OS dialog was cancelled, which is an
      // outcome and not a success: the line clears and nothing claims a file.
      await saveBody(bytes, filename);
    } catch (error) {
      setFailure(reasonOf(error));
    } finally {
      setExporting(false);
    }
  };

  const clear = async () => {
    await api.clearFlows(filter);
    setAsking(null);
    onCleared();
  };

  // Export wins the slot when both run: it is the one whose act the menu is
  // still withholding, so its line is the one that has something to explain.
  const slot = exporting
    ? CAPTURE_COPY.exporting
    : importing
      ? CAPTURE_COPY.importing
      : flowCount(count);

  const acts: CaptureAct[] = [
    {
      id: "export",
      label: CAPTURE_COPY.exportAct,
      glyph: CAPTURE_GLYPH.export,
      busy: exporting,
      disabled: exporting,
      run: () => void exportHar(),
    },
    {
      id: "import",
      label: CAPTURE_COPY.importAct,
      name: CAPTURE_COPY.importName,
      glyph: CAPTURE_GLYPH.import,
      busy: importing,
      run: () => setAsking("import"),
    },
    {
      id: "clear",
      label: CAPTURE_COPY.clearAct,
      glyph: CAPTURE_GLYPH.clear,
      separated: true,
      run: () => setAsking("clear"),
    },
  ];

  return (
    <div className="flex items-center gap-2">
      <span data-testid="capture-slot" className="w-field-sm text-right">
        <TonedText
          tone="muted"
          className="inline-flex items-center justify-end gap-chrome-tight whitespace-nowrap text-control"
        >
          {(exporting || importing) && <Spinner />}
          {slot}
        </TonedText>
      </span>
      <CaptureMenu acts={acts} label={CAPTURE_COPY.trigger} />
      {failure !== null && (
        <div data-testid="export-failed">
          <Notice tone="warn">{failure}</Notice>
        </div>
      )}
      {asking === "clear" && (
        <ClearDialog count={count} onClear={clear} onCancel={() => setAsking(null)} />
      )}
      {asking === "import" && (
        <ImportDialog
          api={api}
          onDone={() => setAsking(null)}
          onBusy={setImporting}
        />
      )}
    </div>
  );
}

// E — the ask. Initial focus goes on the answer that does nothing, so the
// reflexive Enter is the one with no consequence: the act behind it cannot be
// undone, and the ring is memory with nothing persisting it.
function ClearDialog({
  count,
  onClear,
  onCancel,
}: {
  count: number;
  onClear: () => Promise<void>;
  onCancel: () => void;
}) {
  const [failure, setFailure] = useState<string | null>(null);
  const [clearing, setClearing] = useState(false);

  const confirm = () => {
    setFailure(null);
    setClearing(true);
    onClear().catch((error: unknown) => {
      setFailure(reasonOf(error));
      setClearing(false);
    });
  };

  return (
    <ModalShell title={`clear ${flowCount(count)}?`} onCancel={onCancel} testId="clear-ask" dismissLabel="close">
      <Notice tone="warn">{CAPTURE_COPY.clearConsequence}</Notice>
      {failure !== null && (
        <div data-testid="clear-failed">
          <Notice tone="warn">{failure}</Notice>
        </div>
      )}
      <div className="mt-3 flex justify-end gap-chrome-gap">
        <span data-opener="true">
          <Button onClick={onCancel} frame="bare" tone="mute">
            {CAPTURE_COPY.cancel}
          </Button>
        </span>
        <Button onClick={confirm} disabled={clearing}>
          {CAPTURE_COPY.clear}
        </Button>
      </div>
    </ModalShell>
  );
}

// F — the import target. The file picker comes before import because the thing
// that decides what is being imported precedes the thing that imports it. A
// refusal is the server's own sentence, rendered verbatim under the text that
// caused it, and the text stays so it can be edited rather than re-pasted.
//
// The paste offers the file, not the reverse (design run 11, T4): the textarea is
// what opens focused and what the primary button acts on, the picker is the muted
// alternative beside it. So the words lead with the command in both places — the
// title names it first, and the target states its own act rather than opening on
// "…or", which read as an aside to something that was never said.
function ImportDialog({
  api,
  onDone,
  onBusy,
}: {
  api: TrawlerApi;
  onDone: () => void;
  onBusy: (busy: boolean) => void;
}) {
  const [command, setCommand] = useState("");
  const [importing, setImporting] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  // Nothing can cancel a request already sent, so what keeps an abandoned import
  // honest is that a dismissed ask stops listening to it: its answer must not
  // reach back and close whatever the user opened next.
  const listening = useRef(true);
  // Armed in the body, not just at the declaration: StrictMode mounts, unmounts
  // and remounts, so a cleanup-only effect disarms the ref for the whole life of
  // an ask that was never dismissed — and every import then answers into a
  // listener that has stopped listening.
  useEffect(() => {
    listening.current = true;
    return () => void (listening.current = false);
  }, []);

  const dismiss = () => {
    onBusy(false);
    onDone();
  };

  const choose = async () => {
    setFailure(null);
    let file: ChosenFile | null;
    try {
      file = await openFile(["har", "json"]);
    } catch (error) {
      // Opening or reading can fail on its own, before any import is attempted.
      // Without this the rejection is swallowed and the ask just sits there.
      setFailure(reasonOf(error));
      return;
    }
    // A dismissed picker leaves the ask open with nothing claimed — the
    // symmetric case to a cancelled save.
    if (file) await run(() => api.importHar(file.bytes));
  };

  const run = async (call: () => Promise<number>) => {
    setImporting(true);
    onBusy(true);
    try {
      await call();
      if (listening.current) dismiss();
    } catch (error) {
      if (!listening.current) return;
      setFailure(reasonOf(error));
      setImporting(false);
      onBusy(false);
    }
  };

  const importPasted = () => {
    setFailure(null);
    void run(() => api.importCurl(command));
  };

  return (
    <ModalShell title={CAPTURE_COPY.importName} onCancel={dismiss} testId="import-ask" dismissLabel="close">
      <textarea
        data-opener="true"
        data-wire="curl-command"
        aria-label={CAPTURE_COPY.pasteLabel}
        placeholder={CAPTURE_COPY.pastePlaceholder}
        value={command}
        // Read-only rather than disabled: this is the ask's opener, so it holds
        // focus, and disabling it blurs to <body> — outside the dialog, where
        // ModalShell's Esc handler never sees the key and the one exit from a
        // slow import is gone.
        readOnly={importing}
        onChange={(e) => setCommand(e.target.value)}
        className="h-field-sm w-full resize-none rounded-button border border-hairline bg-canvas px-chrome-x py-2 font-mono text-control leading-normal text-ink-body placeholder:text-ink-mute read-only:opacity-disabled focus-visible:outline focus-visible:outline-focus focus-visible:outline-ink-mute focus-visible:outline-offset-focus"
      />
      {importing && (
        <div data-testid="import-running">
          <Notice>
            <span className="inline-flex items-center gap-chrome-gap">
              <Spinner />
              {CAPTURE_COPY.importing}
            </span>
          </Notice>
        </div>
      )}
      {failure !== null && (
        <div data-testid="import-failed">
          <Notice tone="warn">{failure}</Notice>
        </div>
      )}
      <div className="mt-3 flex justify-end gap-chrome-gap">
        <Button onClick={() => void choose()} frame="bare" tone="mute" disabled={importing}>
          {CAPTURE_COPY.choose}
        </Button>
        <Button onClick={importPasted} disabled={importing || command.trim() === ""}>
          {CAPTURE_COPY.import}
        </Button>
      </div>
    </ModalShell>
  );
}

// The API client throws the server's refusal as the message, so this unwraps
// rather than rewords: the sentence the user reads is the one the server wrote.
function reasonOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
