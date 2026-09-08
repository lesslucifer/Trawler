import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { CaptureControls, CAPTURE_COPY } from "./CaptureControls";
import { apiStub } from "./apiStub";
import type { Filter, TrawlerApi } from "./api";
import * as files from "./body";

afterEach(() => vi.restoreAllMocks());

const trigger = () => screen.getByRole("button", { name: CAPTURE_COPY.trigger });
const slot = () => screen.getByTestId("capture-slot");
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

test("clear asks first, names the scope in the title, and focuses the answer that does nothing", () => {
  const clearFlows = vi.fn(async () => 6);
  mount({ clearFlows }, { count: 214 });

  choose(CAPTURE_COPY.clearAct);
  const dialog = screen.getByRole("dialog");
  expect(dialog).toHaveAttribute("aria-label", "clear 214 flows?");
  expect(screen.getByTestId("clear-ask-panel")).toHaveTextContent(CAPTURE_COPY.clearConsequence);
  // The reflexive Enter is the one with no consequence.
  expect(document.activeElement).toBe(screen.getByRole("button", { name: CAPTURE_COPY.cancel }));
  expect(clearFlows).not.toHaveBeenCalled();
});

test("the consequence is the only alarm — the answer itself carries none", () => {
  mount();
  choose(CAPTURE_COPY.clearAct);
  const warn = screen.getByTestId("clear-ask-panel").querySelector('[data-tone="warn"]');
  expect(warn).toHaveTextContent(CAPTURE_COPY.clearConsequence);
  expect(screen.getByRole("button", { name: CAPTURE_COPY.clear }).className).not.toContain("warn");
});

test.each([
  ["cancel", () => fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.cancel }))],
  ["Esc", () => fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" })],
])("changing your mind via %s clears nothing", (_name, dismiss) => {
  const clearFlows = vi.fn(async () => 0);
  const { onCleared } = mount({ clearFlows });
  choose(CAPTURE_COPY.clearAct);
  dismiss();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(clearFlows).not.toHaveBeenCalled();
  expect(onCleared).not.toHaveBeenCalled();
});

test("confirming clears what the filter selects and tells the screen an absence is coming", async () => {
  const clearFlows = vi.fn(async () => 3);
  const { onCleared } = mount({ clearFlows }, { filter: { method: "GET" } });

  choose(CAPTURE_COPY.clearAct);
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.clear }));

  await waitFor(() => expect(clearFlows).toHaveBeenCalledWith({ method: "GET" }));
  await waitFor(() => expect(onCleared).toHaveBeenCalledOnce());
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("a refused clear keeps the ask open and says why", async () => {
  const { onCleared } = mount({ clearFlows: vi.fn().mockRejectedValue(new Error("the store is busy")) });
  choose(CAPTURE_COPY.clearAct);
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.clear }));

  expect(await screen.findByTestId("clear-failed")).toHaveTextContent("the store is busy");
  expect(screen.getByRole("dialog")).toBeInTheDocument();
  expect(onCleared).not.toHaveBeenCalled();
});

test("import opens with focus in the textarea and the file picker before the act", () => {
  mount();
  choose(CAPTURE_COPY.importName);
  const paste = screen.getByRole("textbox", { name: CAPTURE_COPY.pasteLabel });
  expect(document.activeElement).toBe(paste);
  // Multi-line and mono, because a browser-copied command is both.
  expect(paste.tagName).toBe("TEXTAREA");
  expect(paste.className).toContain("font-mono");

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
