import { useState } from "react";
import type { Filter, TrawlerApi } from "./api";
import { openFile, saveBody } from "./body";
import { CaptureMenu, type CaptureAct } from "./CaptureMenu";
import { ModalShell } from "./ModalShell";
import { Button, Notice, TonedText } from "./components/index";

export const CAPTURE_COPY = {
  trigger: "capture actions",
  exportAct: "export as har…",
  importAct: "import…",
  importName: "import a har file or a curl command",
  clearAct: "clear…",
  exporting: "exporting…",
  importing: "importing…",
  clearConsequence: "the capture is not saved anywhere",
  cancel: "cancel",
  clear: "clear",
  choose: "choose a file…",
  import: "import",
  pasteLabel: "a curl command",
  pastePlaceholder: "…or paste a curl command",
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
  const [transfer, setTransfer] = useState<"export" | "import" | null>(null);
  const [asking, setAsking] = useState<"clear" | "import" | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  const exportHar = async () => {
    setFailure(null);
    setTransfer("export");
    try {
      const { bytes, filename } = await api.exportHar(filter);
      // saveBody answers null when the OS dialog was cancelled, which is an
      // outcome and not a success: the line clears and nothing claims a file.
      await saveBody(bytes, filename);
    } catch (error) {
      setFailure(reasonOf(error));
    } finally {
      setTransfer(null);
    }
  };

  const clear = async () => {
    await api.clearFlows(filter);
    setAsking(null);
    onCleared();
  };

  const slot = transfer === null ? flowCount(count) : CAPTURE_COPY[`${transfer}ing`];

  const acts: CaptureAct[] = [
    {
      id: "export",
      label: CAPTURE_COPY.exportAct,
      disabled: transfer === "export",
      run: () => void exportHar(),
    },
    {
      id: "import",
      label: CAPTURE_COPY.importAct,
      name: CAPTURE_COPY.importName,
      run: () => setAsking("import"),
    },
    { id: "clear", label: CAPTURE_COPY.clearAct, separated: true, run: () => setAsking("clear") },
  ];

  return (
    <div className="flex items-center gap-2">
      <span data-testid="capture-slot" className="w-field-sm text-right">
        <TonedText tone="muted" className="whitespace-nowrap text-control">
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
          onBusy={(busy) => setTransfer(busy ? "import" : null)}
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

  const dismiss = () => {
    onBusy(false);
    onDone();
  };

  const choose = async () => {
    setFailure(null);
    const file = await openFile(["har", "json"]);
    // A dismissed picker leaves the ask open with nothing claimed — the
    // symmetric case to a cancelled save.
    if (file) await run(() => api.importHar(file.bytes));
  };

  const run = async (call: () => Promise<number>) => {
    setImporting(true);
    onBusy(true);
    try {
      await call();
      dismiss();
    } catch (error) {
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
        disabled={importing}
        onChange={(e) => setCommand(e.target.value)}
        className="h-field-sm w-full resize-none rounded-button border border-hairline bg-canvas px-chrome-x py-2 font-mono text-control leading-normal text-ink-body placeholder:text-ink-mute focus-visible:outline focus-visible:outline-focus focus-visible:outline-ink-mute focus-visible:outline-offset-focus disabled:opacity-disabled"
      />
      {importing && (
        <div data-testid="import-running">
          <Notice>{CAPTURE_COPY.importing}</Notice>
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
