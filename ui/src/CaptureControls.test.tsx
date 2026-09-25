import { StrictMode } from "react";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { CaptureControls, CAPTURE_COPY, CAPTURE_GLYPH } from "./CaptureControls";
import { apiStub } from "./apiStub";
import type { Filter, TrawlerApi } from "./api";
import * as files from "./body";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => false) }));

afterEach(() => {
  vi.restoreAllMocks();
  vi.mocked(invoke).mockReset().mockResolvedValue(false);
});

const trigger = () => screen.getByRole("button", { name: CAPTURE_COPY.trigger });
const slot = () => screen.getByTestId("capture-slot");
const clearButton = (count = 6) => screen.getByRole("button", { name: `${CAPTURE_COPY.clearAct} ${count} flows` });
const openClear = async (count = 6) => {
  fireEvent.click(clearButton(count));
  await screen.findByRole("dialog");
};
const choose = (label: string) => {
  fireEvent.click(trigger());
  fireEvent.click(screen.getByRole("menuitem", { name: label }));
};

function mount(over: Partial<TrawlerApi> = {}, props: { count?: number; filter?: Filter } = {}) {
  const api = apiStub(over);
  const onCleared = vi.fn();
  render(
    <CaptureControls
      api={api}
      filter={props.filter ?? {}}
      count={props.count ?? 6}
      onCleared={onCleared}
    />,
  );
  return { api, onCleared };
}

test("the slot reads the count the acts are scoped to, in one fixed-width box", () => {
  mount({}, { count: 6 });
  expect(slot()).toHaveTextContent("6 flows");
  // Fixed width, so neither the in-flight line's arrival nor its departure moves
  // the filter field beside it.
  expect(slot().className).toContain("w-field-sm");
  // Sized like the controls it sits among rather than at the inherited body size:
  // the muted, least-important text in the header must not also be its biggest.
  expect(slot().firstElementChild?.className).toContain("text-control");
});

test("one flow is not 1 flows", () => {
  mount({}, { count: 1 });
  expect(slot()).toHaveTextContent("1 flow");
});

test("export sends the live filter, saves under the server's name, and says so meanwhile", async () => {
  let release: (() => void) | undefined;
  const exportHar = vi.fn(
    async () =>
      new Promise<{ bytes: Uint8Array; filename: string }>((resolve) => {
        release = () => resolve({ bytes: new Uint8Array([1, 2]), filename: "trawler-20260908.har" });
      }),
  );
  const saveBody = vi.spyOn(files, "saveBody").mockResolvedValue("/tmp/trawler-20260908.har");
  mount({ exportHar }, { filter: { host: "api.example.com" } });

  choose(CAPTURE_COPY.exportAct);
  await waitFor(() => expect(exportHar).toHaveBeenCalledWith({ host: "api.example.com" }));

  expect(slot()).toHaveTextContent(CAPTURE_COPY.exporting);
  fireEvent.click(trigger());
  expect(screen.getByRole("menuitem", { name: CAPTURE_COPY.exportAct }).getAttribute("aria-disabled")).toBe(
    "true",
  );

  await act(async () => release?.());
  await waitFor(() => expect(slot()).toHaveTextContent("6 flows"));
  expect(saveBody).toHaveBeenCalledWith(new Uint8Array([1, 2]), "trawler-20260908.har");
});

test("a cancelled save clears the line and claims nothing", async () => {
  vi.spyOn(files, "saveBody").mockResolvedValue(null);
  mount();

  choose(CAPTURE_COPY.exportAct);
  await waitFor(() => expect(slot()).toHaveTextContent("6 flows"));
  // No sentence at all about a file the user chose not to write — the one
  // absence deliberately given no wording.
  expect(screen.queryByTestId("export-failed")).toBeNull();
  expect(document.body.textContent).not.toMatch(/\.har/);
});

test("a failed export says so beside the control, in the server's words", async () => {
  mount({
    exportHar: vi.fn().mockRejectedValue(new Error("status must be a number")),
  });
  choose(CAPTURE_COPY.exportAct);
  expect(await screen.findByTestId("export-failed")).toHaveTextContent("status must be a number");
  expect(slot()).toHaveTextContent("6 flows");
});

test("clear asks first, names the scope in the title, and focuses the answer that does nothing", async () => {
  const clearFlows = vi.fn(async () => 6);
  mount({ clearFlows }, { count: 214 });

  await openClear(214);
  const dialog = screen.getByRole("dialog");
  expect(dialog).toHaveAttribute("aria-label", "clear 214 flows?");
  expect(screen.getByTestId("clear-ask-panel")).toHaveTextContent(CAPTURE_COPY.clearConsequence);
  // The reflexive Enter is the one with no consequence.
  expect(document.activeElement).toBe(screen.getByRole("button", { name: CAPTURE_COPY.cancel }));
  expect(clearFlows).not.toHaveBeenCalled();
});

test("the consequence is the only alarm — the answer itself carries none", async () => {
  mount();
  await openClear();
  const warn = screen.getByTestId("clear-ask-panel").querySelector('[data-tone="warn"]');
  expect(warn).toHaveTextContent(CAPTURE_COPY.clearConsequence);
  expect(screen.getByRole("button", { name: CAPTURE_COPY.clear }).className).not.toContain("warn");
});

test.each([
  ["cancel", () => fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.cancel }))],
  ["Esc", () => fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" })],
])("changing your mind via %s clears nothing", async (_name, dismiss) => {
  const clearFlows = vi.fn(async () => 0);
  const { onCleared } = mount({ clearFlows });
  await openClear();
  dismiss();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(clearFlows).not.toHaveBeenCalled();
  expect(onCleared).not.toHaveBeenCalled();
});

test("confirming clears what the filter selects and tells the screen an absence is coming", async () => {
  const clearFlows = vi.fn(async () => 3);
  const { onCleared } = mount({ clearFlows }, { filter: { method: "GET" } });

  await openClear();
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.clear }));

  await waitFor(() => expect(clearFlows).toHaveBeenCalledWith({ method: "GET" }));
  await waitFor(() => expect(onCleared).toHaveBeenCalledOnce());
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("a refused clear keeps the ask open and says why", async () => {
  const { onCleared } = mount({ clearFlows: vi.fn().mockRejectedValue(new Error("the store is busy")) });
  await openClear();
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.clear }));

  expect(await screen.findByTestId("clear-failed")).toHaveTextContent("the store is busy");
  expect(screen.getByRole("dialog")).toBeInTheDocument();
  expect(onCleared).not.toHaveBeenCalled();
});

test("the suppression preference is written only when a checked ask is confirmed", async () => {
  mount({ clearFlows: vi.fn(async () => 1) });
  await openClear();
  fireEvent.click(screen.getByRole("checkbox", { name: CAPTURE_COPY.clearPreference }));
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.cancel }));
  expect(invoke).not.toHaveBeenCalledWith("dismiss_notice", { key: "clear-ask" });

  await openClear();
  fireEvent.click(screen.getByRole("checkbox", { name: CAPTURE_COPY.clearPreference }));
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.clear }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("dismiss_notice", { key: "clear-ask" }));
});

test("a dismissed ask clears immediately with the live filter", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => command === "notice_dismissed");
  const clearFlows = vi.fn(async () => 2);
  const { onCleared } = mount({ clearFlows }, { filter: { host: "api.example.com" } });

  fireEvent.click(clearButton());

  await waitFor(() => expect(clearFlows).toHaveBeenCalledWith({ host: "api.example.com" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(onCleared).toHaveBeenCalledOnce();
});

test("a refused clear with the ask suppressed says so in the header", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => command === "notice_dismissed");
  const { onCleared } = mount({ clearFlows: vi.fn().mockRejectedValue(new Error("the store is busy")) });

  fireEvent.click(clearButton());

  expect(await screen.findByTestId("clear-failed")).toHaveTextContent("the store is busy");
  expect(onCleared).not.toHaveBeenCalled();
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("import opens with focus in the textarea and the file picker before the act", () => {
  mount();
  choose(CAPTURE_COPY.importName);
  const paste = screen.getByRole("textbox", { name: CAPTURE_COPY.pasteLabel });
  expect(document.activeElement).toBe(paste);
  // Multi-line and mono, because a browser-copied command is both.
  expect(paste.tagName).toBe("TEXTAREA");
  expect(paste.className).toContain("font-mono");
  // Field's own multi-line box, not a second hand-made one beside it (DEC-59).
  for (const shared of ["min-h-control", "rounded-button", "border-hairline", "px-control-x", "py-control-y", "text-control"]) {
    expect(paste.className).toContain(shared);
  }
  expect(paste.className).toContain("h-field-sm");

  const words = [...screen.getByTestId("import-ask-panel").querySelectorAll("button")].map(
    (b) => b.textContent,
  );
  expect(words.indexOf(CAPTURE_COPY.choose)).toBeLessThan(words.indexOf(CAPTURE_COPY.import));
});

test("a pasted command goes to the curl door verbatim, and the ask leaves on success", async () => {
  const importCurl = vi.fn(async () => 1);
  mount({ importCurl });
  choose(CAPTURE_COPY.importName);

  const command = "curl 'https://api.example.com/v1/orders' \\\n  -H 'accept: application/json'";
  fireEvent.change(screen.getByRole("textbox", { name: CAPTURE_COPY.pasteLabel }), {
    target: { value: command },
  });
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.import }));

  await waitFor(() => expect(importCurl).toHaveBeenCalledWith(command));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

test("import stays unavailable while there is nothing to import", () => {
  mount();
  choose(CAPTURE_COPY.importName);
  expect(screen.getByRole("button", { name: CAPTURE_COPY.import })).toBeDisabled();

  fireEvent.change(screen.getByRole("textbox", { name: CAPTURE_COPY.pasteLabel }), {
    target: { value: "   " },
  });
  expect(screen.getByRole("button", { name: CAPTURE_COPY.import })).toBeDisabled();
});

test("a chosen file posts its bytes to the HAR door", async () => {
  const bytes = new TextEncoder().encode('{"log":{"entries":[]}}');
  vi.spyOn(files, "openFile").mockResolvedValue({ name: "session.har", bytes });
  const importHar = vi.fn(async () => 4);
  mount({ importHar });

  choose(CAPTURE_COPY.importName);
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.choose }));

  await waitFor(() => expect(importHar).toHaveBeenCalledWith(bytes));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

test("a dismissed file picker leaves the ask open with nothing claimed", async () => {
  const openFile = vi.spyOn(files, "openFile").mockResolvedValue(null);
  const importHar = vi.fn(async () => 0);
  mount({ importHar });

  choose(CAPTURE_COPY.importName);
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.choose }));

  await waitFor(() => expect(openFile).toHaveBeenCalled());
  expect(importHar).not.toHaveBeenCalled();
  expect(screen.getByRole("dialog")).toBeInTheDocument();
  expect(screen.queryByTestId("import-running")).toBeNull();
});

test("a refusal is the server's own sentence, under the text that caused it, which stays", async () => {
  mount({ importCurl: vi.fn().mockRejectedValue(new Error("unsupported flag --data-urlencode")) });
  choose(CAPTURE_COPY.importName);

  const paste = screen.getByRole("textbox", { name: CAPTURE_COPY.pasteLabel });
  fireEvent.change(paste, { target: { value: "curl --data-urlencode 'q=open' http://x/" } });
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.import }));

  const refusal = await screen.findByTestId("import-failed");
  expect(refusal).toHaveTextContent("unsupported flag --data-urlencode");
  expect(refusal.querySelector('[data-tone="warn"]')).not.toBeNull();
  expect(screen.getByRole("dialog")).toBeInTheDocument();
  expect(paste).toHaveValue("curl --data-urlencode 'q=open' http://x/");
});

test("both zones say a transfer is running — the header slot and the surface that has focus", async () => {
  let release: (() => void) | undefined;
  mount({
    importCurl: vi.fn(async () => new Promise<number>((resolve) => (release = () => resolve(1)))),
  });
  choose(CAPTURE_COPY.importName);
  fireEvent.change(screen.getByRole("textbox", { name: CAPTURE_COPY.pasteLabel }), {
    target: { value: "curl http://x/" },
  });
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.import }));

  expect(await screen.findByTestId("import-running")).toHaveTextContent(CAPTURE_COPY.importing);
  expect(slot()).toHaveTextContent(CAPTURE_COPY.importing);
  expect(screen.getByRole("button", { name: CAPTURE_COPY.import })).toBeDisabled();

  await act(async () => release?.());
  await waitFor(() => expect(slot()).toHaveTextContent("6 flows"));
});

// Found by review round 1. The export and the import share one `transfer`
// slot, and the import item is not disabled during an export — so the import
// dialog can be opened mid-export, and merely dismissing it must not clear the
// line belonging to a transfer that is still running.
test("dismissing the import ask mid-export leaves the export's own line alone", async () => {
  let release: (() => void) | undefined;
  mount({
    exportHar: vi.fn(
      async () =>
        new Promise<{ bytes: Uint8Array; filename: string }>((resolve) => {
          release = () => resolve({ bytes: new Uint8Array(), filename: "x.har" });
        }),
    ),
  });
  vi.spyOn(files, "saveBody").mockResolvedValue("/tmp/x.har");

  choose(CAPTURE_COPY.exportAct);
  await waitFor(() => expect(slot()).toHaveTextContent(CAPTURE_COPY.exporting));

  choose(CAPTURE_COPY.importName);
  fireEvent.click(screen.getByRole("button", { name: "close" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(slot()).toHaveTextContent(CAPTURE_COPY.exporting);

  await act(async () => release?.());
  await waitFor(() => expect(slot()).toHaveTextContent("6 flows"));
});

// Found while covering openFile (review round 1). The picker can fail on its
// own, before any import is attempted — a swallowed rejection would leave the
// ask sitting there having done nothing and said nothing.
test("a file that cannot be read says so in the ask, rather than nothing happening", async () => {
  vi.spyOn(files, "openFile").mockRejectedValue(new Error("no longer there"));
  const importHar = vi.fn(async () => 0);
  mount({ importHar });

  choose(CAPTURE_COPY.importName);
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.choose }));

  expect(await screen.findByTestId("import-failed")).toHaveTextContent("no longer there");
  expect(importHar).not.toHaveBeenCalled();
  expect(screen.getByRole("dialog")).toBeInTheDocument();
});

// Round 2 sharpened round 1's finding: guarding the CLEAR was not enough, because
// starting an import mid-export overwrote the export's own in-flight mark, and the
// export's completion then cleared what was left. The two transfers are
// independent and neither may speak for the other.
test("an import running mid-export never speaks for the export", async () => {
  let releaseExport: (() => void) | undefined;
  let releaseImport: (() => void) | undefined;
  const exportHar = vi.fn(
    async () =>
      new Promise<{ bytes: Uint8Array; filename: string }>((resolve) => {
        releaseExport = () => resolve({ bytes: new Uint8Array(), filename: "x.har" });
      }),
  );
  const importCurl = vi.fn(async () => new Promise<number>((resolve) => (releaseImport = () => resolve(1))));
  vi.spyOn(files, "saveBody").mockResolvedValue("/tmp/x.har");
  mount({ exportHar, importCurl });

  choose(CAPTURE_COPY.exportAct);
  await waitFor(() => expect(slot()).toHaveTextContent(CAPTURE_COPY.exporting));

  choose(CAPTURE_COPY.importName);
  fireEvent.change(screen.getByRole("textbox", { name: CAPTURE_COPY.pasteLabel }), {
    target: { value: "curl http://x/" },
  });
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.import }));
  await waitFor(() => expect(importCurl).toHaveBeenCalled());

  // The import finishes first. The export is still running, so its line stays and
  // its act stays unavailable — a second export would otherwise race the first.
  await act(async () => releaseImport?.());
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(slot()).toHaveTextContent(CAPTURE_COPY.exporting);
  fireEvent.click(trigger());
  expect(screen.getByRole("menuitem", { name: CAPTURE_COPY.exportAct }).getAttribute("aria-disabled")).toBe(
    "true",
  );
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

  await act(async () => releaseExport?.());
  await waitFor(() => expect(slot()).toHaveTextContent("6 flows"));
  expect(exportHar).toHaveBeenCalledOnce();
});

// An import the user walked away from must not reach back into the screen: its
// request cannot be cancelled, so the only thing that keeps it honest is that a
// dismissed dialog stops listening to it.
test("an abandoned import never closes the ask the user opened after it", async () => {
  let release: ((n: number) => void) | undefined;
  const importCurl = vi.fn(async () => new Promise<number>((resolve) => (release = resolve)));
  mount({ importCurl });

  choose(CAPTURE_COPY.importName);
  fireEvent.change(screen.getByRole("textbox", { name: CAPTURE_COPY.pasteLabel }), {
    target: { value: "curl http://x/" },
  });
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.import }));
  await waitFor(() => expect(importCurl).toHaveBeenCalled());

  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

  choose(CAPTURE_COPY.importName);
  const reopened = screen.getByRole("dialog");

  await act(async () => release?.(1));
  expect(reopened).toBeInTheDocument();
  expect(slot()).toHaveTextContent("6 flows");
});

const spinners = (within: HTMLElement) => within.querySelectorAll('[data-testid="spinner"]');

const paste = (command: string) =>
  fireEvent.change(screen.getByRole("textbox", { name: CAPTURE_COPY.pasteLabel }), {
    target: { value: command },
  });

// Found by driving the built app by hand. `listening` was armed at its
// declaration and disarmed in a cleanup, so StrictMode's mount → unmount →
// remount disarmed it for good: every import answered into a listener that had
// stopped listening, and the ask it belonged to never closed. Every other test
// here renders unwrapped, which is exactly why none of them saw it — main.tsx
// wraps the app in StrictMode, so this is the tree the user actually runs.
test("a successful import closes its own ask after StrictMode has remounted it", async () => {
  const importCurl = vi.fn(async () => 1);
  render(
    <StrictMode>
      <CaptureControls api={apiStub({ importCurl })} filter={{}} count={6} onCleared={vi.fn()} />
    </StrictMode>,
  );

  choose(CAPTURE_COPY.importName);
  paste("curl http://x/");
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.import }));

  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(slot()).toHaveTextContent("6 flows");
});

// The same tree, the failing half: a refusal must reach the ask it came from and
// be readable there, rather than being swallowed into a dialog frozen mid-import.
test("a refused import still speaks after StrictMode has remounted the ask", async () => {
  const importCurl = vi.fn(async () => {
    throw new Error("not a curl command: starts with \"wget\"");
  });
  render(
    <StrictMode>
      <CaptureControls api={apiStub({ importCurl })} filter={{}} count={6} onCleared={vi.fn()} />
    </StrictMode>,
  );

  choose(CAPTURE_COPY.importName);
  paste("wget http://x/");
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.import }));

  expect(await screen.findByTestId("import-failed")).toHaveTextContent("starts with");
  expect(screen.queryByTestId("import-running")).toBeNull();
});

// Also found by hand: the textarea is the ask's opener, so it holds focus, and
// disabling it during the import blurred focus to <body> — outside the dialog,
// where ModalShell's key handler never sees Esc. A slow import lost its one
// keyboard exit.
//
// jsdom does not blur a focused element when it becomes disabled, so no key
// dispatched here can reproduce that; what it can check is the mechanism the
// browser acts on. The assertion is therefore the tab ring ModalShell itself
// reads: while the import runs, the opener must still be one of the dialog's
// stops, which is what keeps focus — and Esc — inside it. The key below is
// dispatched from wherever focus actually is rather than at the dialog node,
// because dispatching at the node is precisely what hid this.
test("the ask keeps its focus hold, and its esc, while the import is running", async () => {
  let release: ((n: number) => void) | undefined;
  const importCurl = vi.fn(async () => new Promise<number>((resolve) => (release = resolve)));
  mount({ importCurl });

  choose(CAPTURE_COPY.importName);
  paste("curl http://x/");
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.import }));
  await waitFor(() => expect(importCurl).toHaveBeenCalled());

  const panel = screen.getByTestId("import-ask-panel");
  const box = screen.getByRole("textbox", { name: CAPTURE_COPY.pasteLabel });
  expect([...panel.querySelectorAll("textarea:not([disabled])")]).toContain(box);

  const focused = document.activeElement as HTMLElement;
  expect(panel).toContainElement(focused);
  fireEvent.keyDown(focused, { key: "Escape" });

  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await act(async () => release?.(1));
});

// The command stays readable and selectable while it is being imported — the
// point of read-only over disabled is that the text is still there to correct if
// the server refuses it.
test("the pasted command is held rather than emptied while the import runs", async () => {
  let release: ((n: number) => void) | undefined;
  mount({ importCurl: vi.fn(async () => new Promise<number>((resolve) => (release = resolve))) });

  choose(CAPTURE_COPY.importName);
  paste("curl http://x/");
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.import }));

  const box = await screen.findByRole("textbox", { name: CAPTURE_COPY.pasteLabel });
  expect(box).toHaveValue("curl http://x/");
  expect(box).toHaveAttribute("readonly");
  await act(async () => release?.(1));
});

test("a running export turns a ring in the slot, and stops it when the file is written", async () => {
  let release: (() => void) | undefined;
  mount({
    exportHar: vi.fn(
      async () =>
        new Promise<{ bytes: Uint8Array; filename: string }>((resolve) => {
          release = () => resolve({ bytes: new Uint8Array(), filename: "x.har" });
        }),
    ),
  });
  vi.spyOn(files, "saveBody").mockResolvedValue("/tmp/x.har");

  expect(spinners(slot())).toHaveLength(0);
  choose(CAPTURE_COPY.exportAct);
  await waitFor(() => expect(spinners(slot())).toHaveLength(1));

  await act(async () => release?.());
  await waitFor(() => expect(spinners(slot())).toHaveLength(0));
});

// Two zones, one ring each: the header says a transfer is running, and the
// surface holding focus says which one, because the user is looking at the ask.
test("a running import turns a ring in the slot and in the ask it belongs to", async () => {
  let release: ((n: number) => void) | undefined;
  mount({ importCurl: vi.fn(async () => new Promise<number>((resolve) => (release = resolve))) });

  choose(CAPTURE_COPY.importName);
  paste("curl http://x/");
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.import }));

  const running = await screen.findByTestId("import-running");
  expect(spinners(running)).toHaveLength(1);
  expect(running).toHaveTextContent(CAPTURE_COPY.importing);
  expect(spinners(slot())).toHaveLength(1);

  await act(async () => release?.(1));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(spinners(slot())).toHaveLength(0);
});

// The faces are the app's own: ↓ is the save glyph, ↑ is the mark FlowList
// prints on an imported row. Decorative to a reader — the label is the name, so
// the glyph must not reach the accessible one.
test("each act wears its own face without changing what the act is called", () => {
  mount();
  fireEvent.click(trigger());

  const faces = Object.fromEntries(
    screen.getAllByRole("menuitem").map((item) => [item.textContent, item]),
  );
  expect(Object.keys(faces).join(" ")).toContain(CAPTURE_GLYPH.export);
  expect(Object.keys(faces).join(" ")).toContain(CAPTURE_GLYPH.import);
  expect(clearButton().textContent).toBe(CAPTURE_GLYPH.clear);

  // Every face is distinct: a shared glyph would say two acts are the same act.
  const glyphs = Object.values(CAPTURE_GLYPH);
  expect(new Set(glyphs).size).toBe(glyphs.length);

  expect(screen.getByRole("menuitem", { name: CAPTURE_COPY.exportAct })).toBeInTheDocument();
  expect(screen.getByRole("menuitem", { name: CAPTURE_COPY.importName })).toBeInTheDocument();
  expect(screen.queryByRole("menuitem", { name: CAPTURE_COPY.clearAct })).toBeNull();
});

// Reopening the menu mid-transfer is the one way to ask "why is this act
// withheld?", so the answer has to be in the menu and not only in the header.
test("the act that is running turns a ring where its face was", async () => {
  let release: (() => void) | undefined;
  mount({
    exportHar: vi.fn(
      async () =>
        new Promise<{ bytes: Uint8Array; filename: string }>((resolve) => {
          release = () => resolve({ bytes: new Uint8Array(), filename: "x.har" });
        }),
    ),
  });
  vi.spyOn(files, "saveBody").mockResolvedValue("/tmp/x.har");

  choose(CAPTURE_COPY.exportAct);
  await waitFor(() => expect(slot()).toHaveTextContent(CAPTURE_COPY.exporting));

  fireEvent.click(trigger());
  const running = screen.getByRole("menuitem", { name: CAPTURE_COPY.exportAct });
  expect(spinners(running)).toHaveLength(1);
  expect(running.textContent).not.toContain(CAPTURE_GLYPH.export);
  expect(spinners(clearButton())).toHaveLength(0);

  await act(async () => release?.());
});

// Design run 11's T4 — "the paste offers the file, not the reverse". The
// hierarchy was always in the shape: the textarea is what opens focused and what
// the primary button acts on, the picker is the muted alternative beside it. The
// words contradicted it, naming the file first and opening the target on "…or" —
// an aside to something that was never said.
test("the ask's words lead with the command, not with the file it also takes", () => {
  mount();
  choose(CAPTURE_COPY.importName);

  const title = CAPTURE_COPY.importName;
  expect(title.indexOf("curl")).toBeLessThan(title.indexOf("har"));
  expect(screen.getByRole("dialog")).toHaveAccessibleName(title);

  // The primary target states its own act rather than trailing off another's.
  const box = screen.getByRole("textbox", { name: CAPTURE_COPY.pasteLabel });
  expect(box).toHaveAttribute("placeholder", CAPTURE_COPY.pastePlaceholder);
  expect(CAPTURE_COPY.pastePlaceholder).toMatch(/^paste\b/);

  // The file stays reachable and stays second — the menu is the only route to
  // either, so the title has to carry both names however it orders them.
  expect(screen.getByRole("button", { name: CAPTURE_COPY.choose })).toBeInTheDocument();
  expect(title).toContain("har");
});

// Round 6's confirmed finding. The textarea was moved to read-only so that
// disabling it could not blur focus out of the dialog and take Esc with it —
// but "choose a file…" beside it is still a real disabled control, and the file
// picker never holds focus itself (Tauri's dialog is another window, the browser
// fallback's <input> is never in the DOM). So a keyboard user who tabs to that
// button and presses Enter still owns focus when the import starts and disables
// it. jsdom does not blur on disable, so what is asserted is the condition the
// browser acts on: whatever holds focus must still be inside the panel and must
// not be the thing that just went disabled.
test("the file-picker path keeps focus on something that is still enabled", async () => {
  let land: ((n: number) => void) | undefined;
  vi.spyOn(files, "openFile").mockResolvedValue({ name: "c.har", bytes: new Uint8Array([1]) });
  mount({ importHar: vi.fn(async () => new Promise<number>((resolve) => (land = resolve))) });

  choose(CAPTURE_COPY.importName);
  const picker = screen.getByRole("button", { name: CAPTURE_COPY.choose });
  picker.focus();
  expect(picker).toHaveFocus();

  fireEvent.click(picker);
  await waitFor(() => expect(screen.getByTestId("import-running")).toBeInTheDocument());

  const panel = screen.getByTestId("import-ask-panel");
  const focused = document.activeElement as HTMLElement;
  expect(panel).toContainElement(focused);
  expect(focused).not.toBeDisabled();

  // And the exit that focus hold exists to protect still works.
  fireEvent.keyDown(focused, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await act(async () => land?.(1));
});

// Round 7's confirmed finding — the same defect a third time, one dialog over.
// The clear ask's confirm button disables for the whole DELETE, and a browser
// blurs a focused control when that happens, to <body>, outside ModalShell's
// key handler. Cancel staying enabled does not save it: disabling the focused
// element blurs to body, it does not hand focus to a sibling. jsdom will not
// reproduce the blur, so the assertion is the condition the browser acts on.
test("the clear ask keeps its focus hold, and its esc, while the clear runs", async () => {
  let land: (() => void) | undefined;
  mount({ clearFlows: vi.fn(async () => new Promise<number>((resolve) => (land = () => resolve(1)))) });

  await openClear();
  const go = screen.getByRole("button", { name: CAPTURE_COPY.clear });
  go.focus();
  expect(go).toHaveFocus();
  fireEvent.click(go);
  await waitFor(() => expect(go).toBeDisabled());

  const panel = screen.getByTestId("clear-ask-panel");
  const focused = document.activeElement as HTMLElement;
  expect(panel).toContainElement(focused);
  expect(focused).not.toBeDisabled();

  fireEvent.keyDown(focused, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await act(async () => land?.());
});

// Both found by the epic-close review. The two asks each had a window where the
// answer arrived after the dialog it was meant for had gone, and neither the
// slot nor the header was told.
test("an import abandoned while the picker is still open never arms the slot it cannot disarm", async () => {
  let release: ((file: { name: string; bytes: Uint8Array }) => void) | undefined;
  vi.spyOn(files, "openFile").mockReturnValue(new Promise((resolve) => (release = resolve)));
  const importHar = vi.fn(async () => 1);
  mount({ importHar });

  choose(CAPTURE_COPY.importName);
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.choose }));
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

  await act(async () => release?.({ name: "s.har", bytes: new Uint8Array([1]) }));
  expect(importHar).not.toHaveBeenCalled();
  expect(slot()).toHaveTextContent("6 flows");
});

test("a clear that fails after the ask was cancelled still says so in the header", async () => {
  let refuse: ((error: Error) => void) | undefined;
  const clearFlows = vi.fn(async () => new Promise<number>((_, reject) => (refuse = reject)));
  mount({ clearFlows });

  await openClear();
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.clear }));
  await waitFor(() => expect(clearFlows).toHaveBeenCalled());

  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

  await act(async () => refuse?.(new Error("the core went away")));
  expect(screen.getByTestId("clear-failed")).toHaveTextContent("the core went away");
});

test("new request follows import… in the menu, wears ↗, and opens a draft", () => {
  const onNewRequest = vi.fn();
  render(<CaptureControls api={apiStub()} filter={{}} count={6} onCleared={() => {}} onNewRequest={onNewRequest} />);
  fireEvent.click(trigger());

  const items = screen.getAllByRole("menuitem");
  expect(items.map((i) => i.getAttribute("aria-label"))).toEqual([
    CAPTURE_COPY.exportAct,
    CAPTURE_COPY.importName,
    CAPTURE_COPY.newRequestAct,
  ]);
  expect(items[2].textContent).toBe(`${CAPTURE_GLYPH.sent}${CAPTURE_COPY.newRequestAct}`);
  fireEvent.click(items[2]);
  expect(onNewRequest).toHaveBeenCalledOnce();
});
