import { afterEach, beforeAll, beforeEach, describe, expect, it, vi, type MockInstance } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { EditorView } from "@codemirror/view";
import { foldedRanges } from "@codemirror/language";
import * as files from "./body";
import { DataWindowScreen } from "./DataWindowScreen";
import type { DataPayload } from "./dataWindow";
import type { FlowView } from "./api";
import * as codeTools from "./codeTools";
import { loadBeautifier } from "./codeTools";
import { headersPayload } from "./flowPayload";
import { systemZone, time, type TimeReading } from "./timeDecoder";

const ROW = 20;

beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, value: 600 });
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, value: 1200 });
  HTMLElement.prototype.getBoundingClientRect = () =>
    ({ height: ROW, width: 1200, top: 0, left: 0, bottom: ROW, right: 1200, x: 0, y: 0, toJSON() {} }) as DOMRect;
  URL.createObjectURL = vi.fn(() => "blob:data-window");
  URL.revokeObjectURL = vi.fn();
});

const writeText = vi.fn(() => Promise.resolve());
const fetchSpy = vi.fn();
let saved: MockInstance<typeof files.saveBody>;

beforeEach(() => {
  writeText.mockClear();
  Object.assign(navigator, { clipboard: { writeText } });
  vi.stubGlobal("fetch", fetchSpy);
  saved = vi.spyOn(files, "saveBody").mockResolvedValue("/tmp/saved");
});

afterEach(() => {
  vi.unstubAllGlobals();
  saved.mockRestore();
});

const encode = (text: string) => new TextEncoder().encode(text);

function payload(over: Partial<DataPayload> & { text?: string } = {}): DataPayload {
  const { text = '{\n  "plan": "team"\n}', ...rest } = over;
  return { key: "k", title: "response body · GET api.example.com/v2/x", bytes: encode(text), typeHint: "application/json", ...rest };
}

function pickKind(name: string) {
  fireEvent.click(screen.getByRole("button", { name: "treat the data as" }));
  fireEvent.click(screen.getByRole("option", { name }));
}

async function open(handed: DataPayload | null) {
  const take = () => Promise.resolve(handed);
  render(<DataWindowScreen take={take} />);
  if (handed) await screen.findByRole("heading", { name: handed.title });
  else await screen.findByText("nothing was handed to this window");
}

const view = () => EditorView.findFromDOM(screen.getByTestId("data-surface").querySelector(".cm-editor") as HTMLElement)!;
const footer = () => screen.getByRole("contentinfo");

function typeInto(at: number, insert: string) {
  act(() => {
    view().dispatch({ changes: { from: at, insert }, userEvent: "input.type" });
  });
}

function key(k: string, init: KeyboardEventInit = {}) {
  fireEvent.keyDown(document.body, { key: k, metaKey: true, ...init });
}

const saveRow = (name: string) => name.startsWith("save") && !name.includes("original");

function moreItem(name: RegExp | ((name: string) => boolean)) {
  fireEvent.click(screen.getByRole("button", { name: "more" }));
  return within(screen.getByRole("menu", { name: "more" })).getByRole("menuitem", { name });
}

function moreCheck(name: RegExp) {
  fireEvent.click(screen.getByRole("button", { name: "more" }));
  return within(screen.getByRole("menu", { name: "more" })).getByRole("menuitemcheckbox", { name });
}

describe("what the window was handed", () => {
  it("nothing: it says so, and makes no request", async () => {
    await open(null);
    expect(screen.getByText(/opened without data/)).toBeInTheDocument();
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("0 B: none, with nothing to copy or open", async () => {
    await open(payload({ text: "" }));
    expect(screen.getByText("none")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "more" })).toBeNull();
    expect(screen.queryByRole("button", { name: "copy" })).toBeNull();
  });

  it("the window takes the opener's caption as its title", async () => {
    await open(payload());
    expect(document.title).toBe("response body · GET api.example.com/v2/x");
  });
});

describe("the header draws the opener's heading", () => {
  const heading = { what: "request body · raw", method: "POST", host: "www.example.com", path: "/ajax/bnzai", query: "?__a=1&__req=7n&lsd=x" };
  const title = "request body · raw · POST www.example.com/ajax/bnzai?__a=1&__req=7n&lsd=x";

  it("what above; the method, host, path and query on one line, the whole caption on hover", async () => {
    await open(payload({ title, heading }));
    const drawn = screen.getByRole("heading", { name: title });
    expect(drawn).toHaveAttribute("title", title);
    expect(screen.getByText("request body · raw").tagName).toBe("P");
    expect(within(drawn).getByText("POST")).toBeInTheDocument();
    expect(within(drawn).getByText("www.example.com")).toHaveClass("text-ink");
    expect(within(drawn).getByText("/ajax/bnzai")).toHaveClass("text-ink-body");
    expect(within(drawn).getByText("?__a=1&__req=7n&lsd=x")).toHaveClass("text-ink-mute");
    expect(within(drawn).getByText("3 params")).toBeInTheDocument();
    expect(within(drawn).getByText("?__a=1&__req=7n&lsd=x").parentElement).toHaveClass("truncate");
  });

  it("a heading with no query names no params", async () => {
    await open(payload({ heading: { what: "response body", method: "GET", host: "api.example.com", path: "/v2/x", query: "" } }));
    expect(within(screen.getByRole("heading")).getByText("GET")).toBeInTheDocument();
    expect(within(screen.getByRole("heading")).queryByText(/param/)).toBeNull();
  });

  it("without a heading the title is shown whole on one line, even one shaped like a request", async () => {
    await open(payload({ title }));
    const drawn = screen.getByRole("heading", { name: title });
    expect(drawn).toHaveClass("truncate");
    expect(within(drawn).queryByText("POST")).toBeNull();
  });
});

describe("the footer's kind picker", () => {
  it("opens upward, inside a window that never scrolls as a whole", async () => {
    await open(payload());
    fireEvent.click(within(footer()).getByRole("button", { name: "treat the data as" }));
    const list = within(footer()).getByRole("listbox").parentElement!;
    expect(list).toHaveAttribute("data-opens", "above");
    expect(footer().parentElement).toHaveClass("overflow-hidden");
  });
});

describe("a 2 MB JSON payload", () => {
  const big = JSON.stringify({ items: Array.from({ length: 22_000 }, (_, i) => ({ id: i, name: `item ${i}`, tag: "abcdefghij".repeat(6) })) }, null, 2);

  it("renders whole, editable, its kind detected from the hint, with size and lines in the status bar", async () => {
    expect(big.length).toBeGreaterThan(2_000_000);
    await open(payload({ text: big }));
    expect(view().state.doc.length).toBe(big.length);
    expect(view().state.readOnly).toBe(false);
    expect(screen.getByTestId("detected")).toHaveTextContent("from application/json");
    expect(footer()).toHaveTextContent(files.humanBytes(big.length));
    expect(footer()).toHaveTextContent(`${big.split("\n").length} lines`);
    expect(screen.queryByTestId("modified")).toBeNull();
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

describe("the working copy", () => {
  it("a hand edit is modified · 1 edit; ⌘Z and ⇧⌘Z walk it", async () => {
    await open(payload());
    typeInto(12, "x");
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 1 edit");
    key("z");
    expect(screen.queryByTestId("modified")).toBeNull();
    expect(view().state.doc.toString()).toBe('{\n  "plan": "team"\n}');
    key("z", { shiftKey: true });
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 1 edit");
  });

  it("the line count and size catch up once typing pauses", async () => {
    await open(payload());
    expect(footer()).toHaveTextContent("3 lines");
    typeInto(1, "\n\n");
    await waitFor(() => expect(footer()).toHaveTextContent("5 lines"));
    expect(footer()).toHaveTextContent(files.humanBytes(encode('{\n\n\n  "plan": "team"\n}').length));
  });

  it("keystrokes in one input group are one step", async () => {
    await open(payload());
    typeInto(12, "x");
    typeInto(13, "y");
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 1 edit");
  });

  it("restore original returns the exact handed-in bytes", async () => {
    const bytes = encode("café");
    await open(payload({ bytes, typeHint: "text/plain", text: undefined }));
    typeInto(0, "edited ");
    fireEvent.click(moreItem(/^restore original/));
    expect(screen.queryByTestId("modified")).toBeNull();

    key("s");
    expect(saved).toHaveBeenLastCalledWith(bytes, "response-body-GET-api.example.com-v2-x.txt");
    expect(saved.mock.lastCall![0]).toBe(bytes);
  });

  it("⋯ disables undo, redo and restore with a reason until there is something to do", async () => {
    await open(payload());
    expect(moreItem(/^undo/)).toHaveTextContent("nothing to undo");
    expect(screen.getByRole("menuitem", { name: /^redo/ })).toHaveTextContent("nothing to redo");
    expect(screen.getByRole("menuitem", { name: /^restore original/ })).toHaveTextContent("unmodified");
  });
});

describe("bytes the text cannot hold", () => {
  it("data that is not valid UTF-8 stays read-only and says why, so no edit can rewrite the bytes around it", async () => {
    const bytes = new Uint8Array([0x61, 0xe9, 0x62]);
    await open(payload({ bytes, typeHint: "text/plain", text: undefined }));
    expect(screen.getByTestId("detected")).toHaveTextContent("from text/plain");
    pickKind("text");
    expect(screen.getByText(/not valid UTF-8/)).toBeInTheDocument();
    expect(view().state.readOnly).toBe(true);
    expect(footer()).toHaveTextContent("read-only");
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["save⌘S"]);
    fireEvent.click(screen.getByRole("menuitem"));
    expect(saved.mock.lastCall![0]).toBe(bytes);
  });

  it("an edit keeps the CRLF line breaks it did not touch", async () => {
    const bytes = encode("one\r\ntwo\r\nthree");
    await open(payload({ bytes, typeHint: "text/plain", text: undefined }));
    typeInto(0, "x");
    key("s");
    expect(new TextDecoder().decode(saved.mock.lastCall![0])).toBe("xone\r\ntwo\r\nthree");
  });

  it("a CR-only text counts its lines by its own break", async () => {
    await open(payload({ bytes: encode("a\rb\rc"), typeHint: "text/plain", text: undefined }));
    expect(footer()).toHaveTextContent("3 lines");
  });

  it("mixed line breaks stay read-only and say why", async () => {
    await open(payload({ bytes: encode("a\r\nb\nc"), typeHint: "text/plain", text: undefined }));
    expect(screen.getByText(/mixed line breaks/)).toBeInTheDocument();
    expect(view().state.readOnly).toBe(true);
  });

  it("a byte-order mark survives an edit", async () => {
    const bytes = new Uint8Array([0xef, 0xbb, 0xbf, ...encode("abc")]);
    await open(payload({ bytes, typeHint: "text/plain", text: undefined }));
    typeInto(view().state.doc.length, "d");
    key("s");
    expect(Array.from(saved.mock.lastCall![0])).toEqual([0xef, 0xbb, 0xbf, ...encode("abcd")]);
  });
});

describe("copy and save", () => {
  it("copy and save deliver what is on screen; copy and save original the handed-in bytes", async () => {
    const handed = payload();
    await open(handed);
    typeInto(12, "x");
    const edited = view().state.doc.toString();

    fireEvent.click(screen.getByRole("button", { name: "copy" }));
    expect(writeText).toHaveBeenLastCalledWith(edited);

    fireEvent.click(moreItem(/^copy original/));
    expect(writeText).toHaveBeenLastCalledWith('{\n  "plan": "team"\n}');

    fireEvent.click(moreItem(saveRow));
    expect(saved).toHaveBeenLastCalledWith(encode(edited), "response-body-GET-api.example.com-v2-x.json");

    fireEvent.click(moreItem(/^save original/));
    expect(saved.mock.lastCall![0]).toBe(handed.bytes);
    expect(saved.mock.lastCall![1]).toBe("response-body-GET-api.example.com-v2-x-original.json");
  });
});

describe("a copy from a menu or a key says how it went", () => {
  it("copied, or copy failed, in the status bar", async () => {
    await open(payload());
    fireEvent.click(moreItem(/^copy original/));
    expect(await within(footer()).findByText("copied")).toHaveClass("text-ok");

    writeText.mockImplementationOnce(() => Promise.reject(new Error("denied")));
    fireEvent.click(moreItem(/^copy original/));
    expect(await within(footer()).findByText("copy failed")).toHaveClass("text-warn");
  });
});

describe("a 300 KiB form payload", () => {
  const form = Array.from({ length: 5000 }, (_, i) => `field_${i}=${"v".repeat(60)}`).join("&");

  it("every pair reachable, the last one whole; an edit and a removal are steps", async () => {
    expect(form.length).toBeGreaterThan(300 * 1024);
    await open(payload({ text: form, typeHint: "application/x-www-form-urlencoded" }));
    expect(footer()).toHaveTextContent("5000 pairs");

    const scroller = screen.getByTestId("data-surface");
    act(() => {
      scroller.scrollTop = Number.MAX_SAFE_INTEGER;
      fireEvent.scroll(scroller);
    });
    const rows = screen.getAllByTestId("form-pair");
    const last = rows[rows.length - 1];
    expect(within(last).getByText("field_4999")).toBeInTheDocument();
    expect(within(last).getByText("v".repeat(60))).toBeInTheDocument();

    fireEvent.click(within(last).getByRole("button", { name: "edit the value of pair 5000" }));
    const field = screen.getByRole("textbox", { name: "value" });
    fireEvent.change(field, { target: { value: "changed" } });
    fireEvent.keyDown(field, { key: "Enter", metaKey: true });
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 1 edit");

    fireEvent.contextMenu(screen.getAllByTestId("form-pair")[0], { clientX: 50, clientY: 60 });
    const menu = screen.getByRole("menu", { name: "the selection" });
    fireEvent.click(within(menu).getByRole("menuitem", { name: /^remove the pair/ }));
    expect(footer()).toHaveTextContent("4999 pairs");
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 2 edits");
  });

  it("a row's right-click copies its value", async () => {
    await open(payload({ text: "a=1&b=two%20words", typeHint: "application/x-www-form-urlencoded" }));
    fireEvent.contextMenu(screen.getAllByTestId("form-pair")[1], { clientX: 5, clientY: 5 });
    fireEvent.click(screen.getByRole("menuitem", { name: /^copy the value/ }));
    expect(writeText).toHaveBeenLastCalledWith("two words");
  });

  it("⌘C copies the selected row's value when no text is selected", async () => {
    await open(payload({ text: "a=1&b=two", typeHint: "application/x-www-form-urlencoded" }));
    fireEvent.click(screen.getAllByTestId("form-pair")[1]);
    key("c");
    expect(writeText).toHaveBeenLastCalledWith("two");
  });

  it("⌘⌫ removes the selected row's pair", async () => {
    await open(payload({ text: "a=1&b=2", typeHint: "application/x-www-form-urlencoded" }));
    fireEvent.click(screen.getAllByTestId("form-pair")[0]);
    key("Backspace");
    expect(footer()).toHaveTextContent("1 pair");
  });
});

describe("a 1 MiB binary payload", () => {
  const bytes = new Uint8Array(1024 * 1024).map((_, i) => (i * 7) & 0xff);

  it("the whole dump, read-only, ⋯ holding only save, copy delivering the dump", async () => {
    await open(payload({ bytes, typeHint: "application/octet-stream", text: undefined }));
    expect(screen.getByTestId("detected")).toHaveTextContent("from application/octet-stream");
    expect(footer()).toHaveTextContent("read-only");
    expect(footer()).toHaveTextContent("1.0 MiB");

    const scroller = screen.getByTestId("data-surface");
    act(() => {
      scroller.scrollTop = Number.MAX_SAFE_INTEGER;
      fireEvent.scroll(scroller);
    });
    const rows = screen.getAllByTestId("hex-row");
    expect(rows[rows.length - 1]).toHaveTextContent(/^000ffff0/);

    fireEvent.click(screen.getByRole("button", { name: "more" }));
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["save⌘S"]);
    fireEvent.click(screen.getByRole("menuitem"));
    expect(saved.mock.lastCall![0]).toBe(bytes);

    fireEvent.click(screen.getByRole("button", { name: "copy" }));
    expect(writeText).toHaveBeenLastCalledWith(files.hexDump(bytes, bytes.length));
  });
});

describe("the kind", () => {
  it("can be overridden from the status bar, which then says both", async () => {
    await open(payload());
    fireEvent.click(screen.getByRole("button", { name: "treat the data as" }));
    fireEvent.click(screen.getByRole("option", { name: "text" }));
    expect(screen.getByTestId("detected")).toHaveTextContent("read JSON from application/json");
    fireEvent.click(screen.getByRole("button", { name: "treat the data as" }));
    fireEvent.click(screen.getByRole("option", { name: "JSON" }));
    expect(screen.getByTestId("detected")).toHaveTextContent(/^from application\/json$/);
  });

  it("the picker offers nine kinds, the pick overrides, and the detected kind restores the wording", async () => {
    await open(payload({ typeHint: "text/plain" }));
    fireEvent.click(screen.getByRole("button", { name: "treat the data as" }));
    expect(screen.getAllByRole("option").map((option) => option.textContent?.replace("✓", ""))).toEqual([
      "JSON", "form", "XML", "HTML", "JavaScript", "CSS", "text", "hex", "image",
    ]);
    fireEvent.click(screen.getByRole("option", { name: "text" }));
    expect(screen.getByTestId("detected")).toHaveTextContent("read JSON from its bytes");
    pickKind("JSON");
    expect(screen.getByTestId("detected")).toHaveTextContent(/^read from its bytes · served as text\/plain$/);
  });

  it.each([
    ["JSON served as text/plain", '{"a":1}', "text/plain", "read from its bytes · served as text/plain"],
    ["JSON with no label", '{"a":1}', undefined, "read from its bytes"],
    ["HTML served as application/octet-stream", "<!doctype html><p>x</p>", "application/octet-stream", "read from its bytes · served as application/octet-stream"],
    ["broken JSON under text/plain", "{oops", "text/plain", "from text/plain"],
  ])("%s: the status bar says what decided", async (_name, text, typeHint, said) => {
    await open(payload({ text, typeHint }));
    expect(screen.getByTestId("detected")).toHaveTextContent(new RegExp(`^${said.replace(/[/]/g, "\\/")}$`));
  });

  it("JSON served as text/plain opens as JSON, with its tools", async () => {
    await open(payload({ text: '{"a":1}', typeHint: "text/plain" }));
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    expect(screen.getByRole("group", { name: "indent" })).toBeInTheDocument();
  });

  it("HTML served as application/octet-stream opens with the markup tools; broken JSON stays text", async () => {
    await open(payload({ text: "<!doctype html><html><body><p>x</p></body></html>", typeHint: "application/octet-stream" }));
    fireEvent.click(screen.getByRole("button", { name: "treat the data as" }));
    expect(screen.getByRole("option", { name: "HTML", selected: true })).toBeInTheDocument();
  });

  it("an image offers fit and actual size in ⋯", async () => {
    await open(payload({ bytes: new Uint8Array([0x89, 0x50, 0x4e, 0x47]), typeHint: "image/png", text: undefined }));
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    fireEvent.click(screen.getByRole("menuitemradio", { name: "actual" }));
    expect(screen.getByRole("menuitemradio", { name: "actual" })).toHaveAttribute("aria-checked", "true");
    expect(screen.queryByRole("button", { name: "copy" })).toBeNull();
  });
});

describe("the right-click menu on a text surface", () => {
  it("copy waits for a selection, then copies it", async () => {
    await open(payload());
    fireEvent.contextMenu(screen.getByTestId("data-surface"), { clientX: 10, clientY: 10 });
    expect(screen.getByRole("menuitem", { name: /^copy(?! the)/ })).toHaveTextContent("select some text");
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

    act(() => {
      view().dispatch({ selection: { anchor: 5, head: 9 } });
    });
    await waitFor(() => expect(footer()).toHaveTextContent("selection 4 characters"));
    fireEvent.contextMenu(screen.getByTestId("data-surface"), { clientX: 10, clientY: 10 });
    fireEvent.click(screen.getByRole("menuitem", { name: /^copy(?! the)/ }));
    expect(writeText).toHaveBeenLastCalledWith("plan");
  });
});

const findBox = () => screen.getByRole("textbox", { name: "find" });
const findCount = () => screen.getByTestId("find-count");
const doc = () => view().state.doc.toString();

function findFor(query: string) {
  fireEvent.change(findBox(), { target: { value: query } });
}

function chooseIn(row: string, choice: string) {
  fireEvent.click(screen.getByRole("button", { name: "more" }));
  const group = within(screen.getByRole("menu", { name: "more" })).getByRole("group", { name: row });
  fireEvent.click(within(group).getByRole("menuitemradio", { name: choice }));
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
}

function foldCount() {
  let n = 0;
  foldedRanges(view().state).between(0, view().state.doc.length, () => {
    n += 1;
  });
  return n;
}

describe("find over a 2 MB JSON document", () => {
  const big = JSON.stringify({ items: Array.from({ length: 22_000 }, (_, i) => ({ id: i, name: `item ${i}` })) }, null, 2);

  it("⌘F focuses it; it counts every match and steps by Enter, ⇧Enter, ⌘G and ⇧⌘G, wrapping", async () => {
    expect(big.length).toBeGreaterThan(1_000_000);
    await open(payload({ text: big }));
    key("f");
    expect(findBox()).toHaveFocus();
    findFor("ITEM 2199");
    expect(findCount()).toHaveTextContent("1 of 11");
    fireEvent.keyDown(findBox(), { key: "Enter" });
    expect(findCount()).toHaveTextContent("2 of 11");
    fireEvent.keyDown(findBox(), { key: "Enter", shiftKey: true });
    fireEvent.keyDown(findBox(), { key: "Enter", shiftKey: true });
    expect(findCount()).toHaveTextContent("11 of 11");
    key("g");
    expect(findCount()).toHaveTextContent("1 of 11");
    key("g", { shiftKey: true });
    expect(findCount()).toHaveTextContent("11 of 11");
    fireEvent.click(screen.getByRole("button", { name: "next match" }));
    expect(findCount()).toHaveTextContent("1 of 11");
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("Esc clears it and returns to the surface", async () => {
    await open(payload());
    findFor("plan");
    expect(findCount()).toHaveTextContent("1 of 1");
    fireEvent.keyDown(findBox(), { key: "Escape" });
    expect(findBox()).toHaveValue("");
    expect(findCount()).toHaveTextContent("");
    expect(view().hasFocus).toBe(true);
  });

  it("says no match, and its arrows wait for one", async () => {
    await open(payload());
    findFor("absent");
    expect(findCount()).toHaveTextContent("no match");
    expect(screen.getByRole("button", { name: "next match" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "previous match" })).toBeDisabled();
  });

  it("a hand edit is counted at once", async () => {
    await open(payload());
    findFor("team");
    expect(findCount()).toHaveTextContent("1 of 1");
    typeInto(1, '"team": 1,');
    expect(findCount()).toHaveTextContent("1 of 2");
  });

  it("finds in a CRLF document at the surface's own positions", async () => {
    await open(payload({ text: '{\r\n  "a": "x",\r\n  "b": "x"\r\n}' }));
    findFor('"x"');
    expect(findCount()).toHaveTextContent("1 of 2");
    const marked = [...screen.getByTestId("data-surface").querySelectorAll(".cm-content .bg-wash-select, .cm-content .bg-wash-active")];
    expect(marked.map((m) => m.textContent)).toEqual(['"x"', '"x"']);
  });

  it("is offered on every kind but an image", async () => {
    await open(payload({ bytes: new Uint8Array([0x89, 0x50, 0x4e, 0x47]), typeHint: "image/png", text: undefined }));
    expect(screen.queryByRole("textbox", { name: "find" })).toBeNull();
  });
});

describe("indent and sort keys", () => {
  const text = '{"b": {"y": 1, "x": 2}, "a": [1, 2]}';

  it("each is a named step; undo walks them; the indent chosen is shown", async () => {
    await open(payload({ text }));
    chooseIn("indent", "4");
    expect(doc()).toBe(JSON.stringify(JSON.parse(text), null, 4));
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 1 edit");
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    expect(within(screen.getByRole("group", { name: "indent" })).getByRole("menuitemradio", { name: "4" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    fireEvent.click(screen.getByRole("menuitem", { name: /^sort keys/ }));
    expect(doc()).toBe('{\n    "a": [\n        1,\n        2\n    ],\n    "b": {\n        "x": 2,\n        "y": 1\n    }\n}');
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 2 edits");
    key("z");
    expect(doc()).toBe(JSON.stringify(JSON.parse(text), null, 4));
    key("z");
    expect(doc()).toBe(text);
    expect(screen.queryByTestId("modified")).toBeNull();
  });

  it("minified and sorted, a number past 2^53 and a repeated key stay as written", async () => {
    await open(payload({ text: '{ "id": 9007199254740993, "a": 1, "a": 2 }' }));
    fireEvent.click(moreItem(/^sort keys/));
    chooseIn("indent", "minified");
    expect(doc()).toBe('{"a":1,"a":2,"id":9007199254740993}');
    fireEvent.click(moreItem(/^restore original/));
    key("s");
    expect(saved.mock.lastCall![0]).toEqual(encode('{ "id": 9007199254740993, "a": 1, "a": 2 }'));
  });

  it("sort keys changes the key order only, keeping an 8-space indent", async () => {
    await open(payload({ text: '{\n        "b": 1,\n        "a": [\n                2\n        ]\n}' }));
    fireEvent.click(moreItem(/^sort keys/));
    expect(doc()).toBe('{\n        "a": [\n                2\n        ],\n        "b": 1\n}');
  });

  it("a CRLF document is laid out with CRLF", async () => {
    await open(payload({ text: '{"a":1}\r\n' }));
    chooseIn("indent", "2");
    key("s");
    expect(new TextDecoder().decode(saved.mock.lastCall![0])).toBe('{\r\n  "a": 1\r\n}\r\n');
  });

  it("a working copy that does not parse says where; indent and sort keys wait for it", async () => {
    await open(payload({ text: '{\n  "a": \n}' }));
    expect(screen.getByText(/^not valid JSON at line 3, column 1: expected a value/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    const indent = screen.getByRole("group", { name: "indent" });
    expect(indent).toHaveAttribute("aria-disabled", "true");
    expect(indent).toHaveTextContent("needs valid JSON");
    expect(screen.getByRole("menuitem", { name: /^sort keys/ })).toHaveTextContent("needs valid JSON");
    expect(screen.getByRole("group", { name: "fold to" })).not.toHaveAttribute("aria-disabled");
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    findFor("a");
    expect(findCount()).toHaveTextContent("1 of 1");
  });

  it("an edit that breaks the JSON is named once typing pauses, and mending it clears the notice", async () => {
    await open(payload());
    typeInto(1, "x");
    await waitFor(() => expect(screen.getByText(/^not valid JSON at line 1, column 2/)).toBeInTheDocument());
    key("z");
    await waitFor(() => expect(screen.queryByText(/^not valid JSON/)).toBeNull());
  });

  it("a read-only JSON text offers fold to, never a reformat", async () => {
    await open(payload({ bytes: new Uint8Array([0x7b, 0x22, 0x61, 0x22, 0x3a, 0x22, 0xe9, 0x22, 0x7d]), text: undefined }));
    pickKind("JSON");
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    expect(screen.queryByRole("group", { name: "indent" })).toBeNull();
    expect(screen.queryByRole("menuitem", { name: /^sort keys/ })).toBeNull();
    expect(screen.getByRole("group", { name: "fold to" })).toBeInTheDocument();
  });
});

describe("fold to", () => {
  const nested = JSON.stringify({ a: { b: { c: [1, 2] } }, d: [{ e: 1 }] }, null, 2);

  it("folds from ⋯ to a depth, all, and none, showing the choice until the text changes", async () => {
    await open(payload({ text: nested }));
    chooseIn("fold to", "1");
    expect(foldCount()).toBe(2);
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    expect(screen.getByRole("menuitemradio", { name: "1" })).toHaveAttribute("aria-checked", "true");
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    chooseIn("fold to", "none");
    expect(foldCount()).toBe(0);
    chooseIn("fold to", "all");
    expect(foldCount()).toBeGreaterThan(2);
    typeInto(0, " ");
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    expect(within(screen.getByRole("group", { name: "fold to" })).queryByRole("menuitemradio", { checked: true })).toBeNull();
  });

  it("is offered on XML too", async () => {
    await open(payload({ text: "<a>\n  <b/>\n</a>", typeHint: "application/xml" }));
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    expect(screen.getByRole("group", { name: "fold to" })).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "indent" })).toBeNull();
  });
});

describe("copy a node's path or value", () => {
  const text = '{\n  "user": {\n    "roles": ["admin", "ops"],\n    "odd key": {"n": 9007199254740993}\n  }\n}';

  function rightClickAt(offset: number) {
    vi.spyOn(view(), "posAtCoords").mockReturnValue(offset);
    fireEvent.contextMenu(screen.getByTestId("data-surface"), { clientX: 40, clientY: 40 });
  }

  it("from the right-click, at the node under the pointer", async () => {
    await open(payload({ text }));
    rightClickAt(text.indexOf('"ops"') + 1);
    fireEvent.click(screen.getByRole("menuitem", { name: /^copy the path/ }));
    expect(writeText).toHaveBeenLastCalledWith("$.user.roles[1]");
    await waitFor(() => expect(footer()).toHaveTextContent("copied"));

    rightClickAt(text.indexOf("9007"));
    fireEvent.click(screen.getByRole("menuitem", { name: /^copy the value/ }));
    expect(writeText).toHaveBeenLastCalledWith("9007199254740993");

    rightClickAt(text.indexOf('"odd key"') + 2);
    fireEvent.click(screen.getByRole("menuitem", { name: /^copy the path/ }));
    expect(writeText).toHaveBeenLastCalledWith('$.user["odd key"]');
    rightClickAt(text.indexOf('"odd key"') + 2);
    fireEvent.click(screen.getByRole("menuitem", { name: /^copy the value/ }));
    expect(writeText).toHaveBeenLastCalledWith('{"n": 9007199254740993}');
  });

  it("from the keys, at the node under the cursor; a string's value is its text", async () => {
    await open(payload({ text }));
    act(() => {
      view().dispatch({ selection: { anchor: text.indexOf("admin") } });
    });
    fireEvent.keyDown(view().contentDOM, { key: "ç", code: "KeyC", metaKey: true, altKey: true });
    expect(writeText).toHaveBeenLastCalledWith("$.user.roles[0]");
    fireEvent.keyDown(view().contentDOM, { key: "Ç", code: "KeyC", metaKey: true, altKey: true, shiftKey: true });
    expect(writeText).toHaveBeenLastCalledWith("admin");
  });

  it("a container's value keeps the document's CRLF", async () => {
    await open(payload({ text: '{\r\n  "a": [\r\n    1\r\n  ]\r\n}' }));
    rightClickAt(9);
    fireEvent.click(screen.getByRole("menuitem", { name: /^copy the value/ }));
    expect(writeText).toHaveBeenLastCalledWith("[\r\n    1\r\n  ]");
  });

  it("waits for the JSON to parse, and is never offered on text", async () => {
    await open(payload({ text: '{"a": }' }));
    rightClickAt(2);
    expect(screen.getByRole("menuitem", { name: /^copy the path/ })).toHaveTextContent("needs valid JSON");
    expect(screen.getByRole("menuitem", { name: /^copy the value/ })).toHaveTextContent("needs valid JSON");
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    cleanup();
    await open(payload({ text: "plain words", typeHint: "text/plain" }));
    fireEvent.contextMenu(screen.getByTestId("data-surface"), { clientX: 10, clientY: 10 });
    expect(screen.queryByRole("menuitem", { name: /^copy the path/ })).toBeNull();
  });
});

const FORM_HINT = "application/x-www-form-urlencoded";
const rowsShown = () => screen.getAllByTestId("form-pair");

describe("find in a 300 KiB form", () => {
  const form = Array.from({ length: 5000 }, (_, i) => `field_${i}=${i % 1000 === 7 ? "Needle" : "v".repeat(60)}`).join("&");

  it("counts over keys and values, marks them, steps by every key and brings the row into view", async () => {
    expect(form.length).toBeGreaterThan(300 * 1024);
    HTMLElement.prototype.scrollTo = function (this: HTMLElement, to?: ScrollToOptions | number) {
      this.scrollTop = typeof to === "object" ? (to.top ?? 0) : 0;
    } as HTMLElement["scrollTo"];
    const tall = vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockReturnValue(10_000_000);
    await open(payload({ text: form, typeHint: FORM_HINT }));
    key("f");
    expect(findBox()).toHaveFocus();
    findFor("needle");
    expect(findCount()).toHaveTextContent("1 of 5");
    expect(document.querySelector("mark.bg-wash-active")).toHaveTextContent("Needle");
    fireEvent.keyDown(findBox(), { key: "Enter", shiftKey: true });
    expect(findCount()).toHaveTextContent("5 of 5");
    key("g");
    expect(findCount()).toHaveTextContent("1 of 5");
    findFor("field_4999");
    expect(findCount()).toHaveTextContent("1 of 1");
    fireEvent.scroll(screen.getByTestId("data-surface"));
    expect(rowsShown().some((row) => row.textContent?.includes("field_4999"))).toBe(true);
    delete (HTMLElement.prototype as Partial<HTMLElement>).scrollTo;
    tall.mockRestore();
  });

  it("recounts after an edit", async () => {
    await open(payload({ text: "a=x&b=y", typeHint: FORM_HINT }));
    findFor("x");
    expect(findCount()).toHaveTextContent("1 of 1");
    fireEvent.click(screen.getByRole("button", { name: "edit the value of pair 2" }));
    const field = screen.getByRole("textbox", { name: "value" });
    fireEvent.change(field, { target: { value: "xx" } });
    fireEvent.keyDown(field, { key: "Enter", metaKey: true });
    expect(findCount()).toHaveTextContent("1 of 3");
  });

  it("Esc clears it", async () => {
    await open(payload({ text: "a=x", typeHint: FORM_HINT }));
    findFor("x");
    fireEvent.keyDown(findBox(), { key: "Escape" });
    expect(findBox()).toHaveValue("");
    expect(findBox()).not.toHaveFocus();
    expect(document.querySelector("mark")).toBeNull();
  });
});

describe("the form's ⋯", () => {
  const form = "b=2&a=1&b=&a=%ZZ";

  it("sort by key is one step, the bad escape kept as sent; undo returns the wire order", async () => {
    await open(payload({ text: form, typeHint: FORM_HINT }));
    fireEvent.click(moreItem(/^sort by key/));
    expect(rowsShown().map((row) => within(row).getAllByRole("button")[0].textContent)).toEqual(["a", "a", "b", "b"]);
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 1 edit");
    fireEvent.click(screen.getByRole("button", { name: "copy" }));
    expect(writeText).toHaveBeenLastCalledWith("a=1&a=%ZZ&b=2&b=");
    key("z");
    fireEvent.click(screen.getByRole("button", { name: "copy" }));
    expect(writeText).toHaveBeenLastCalledWith(form);
  });

  it("copy as JSON keeps repeated keys and empty values, and says copied", async () => {
    await open(payload({ text: form, typeHint: FORM_HINT }));
    fireEvent.click(moreItem(/^copy as JSON/));
    expect(writeText).toHaveBeenLastCalledWith('{\n  "b": [\n    "2",\n    ""\n  ],\n  "a": [\n    "1",\n    "%ZZ"\n  ]\n}');
    await waitFor(() => expect(footer()).toHaveTextContent("copied"));
  });

  it("show encoded puts the query string on the text surface, editable and findable; sort waits; the table reads the edit", async () => {
    await open(payload({ text: form, typeHint: FORM_HINT }));
    fireEvent.click(moreCheck(/^show encoded/));
    expect(doc()).toBe(form);
    findFor("%ZZ");
    expect(findCount()).toHaveTextContent("1 of 1");
    typeInto(0, "c=3&");
    expect(footer()).toHaveTextContent("1 line");

    fireEvent.click(screen.getByRole("button", { name: "more" }));
    const menu = screen.getByRole("menu", { name: "more" });
    expect(within(menu).getByRole("menuitemcheckbox", { name: /^show encoded/ })).toHaveAttribute("aria-checked", "true");
    expect(within(menu).getByRole("menuitem", { name: /^sort by key/ })).toHaveAttribute("aria-disabled", "true");
    expect(within(menu).getByRole("menuitem", { name: /^sort by key/ })).toHaveTextContent("shown encoded");
    expect(within(menu).getByRole("menuitemcheckbox", { name: /^wrap long lines/ })).toBeInTheDocument();
    fireEvent.click(within(menu).getByRole("menuitemcheckbox", { name: /^show encoded/ }));

    expect(rowsShown()).toHaveLength(5);
    expect(within(rowsShown()[0]).getByText("c")).toBeInTheDocument();
    expect(footer()).toHaveTextContent("5 pairs");
  });
});

describe("find in a 1 MiB binary payload", () => {
  const bytes = new Uint8Array(1024 * 1024);
  bytes.set([0xde, 0xad], 15);
  bytes.set(new TextEncoder().encode("GET /"), 700_000);

  it("reads hex pairs as bytes and anything else as text, says which, and marks a match across two rows", async () => {
    await open(payload({ bytes, typeHint: "application/octet-stream", text: undefined }));
    key("f");
    expect(findBox()).toHaveFocus();
    findFor("de ad");
    expect(findCount()).toHaveTextContent("1 of 1 · as bytes");
    const marked = [...screen.getByTestId("data-surface").querySelectorAll("[data-offset].bg-wash-active")];
    expect(marked.map((cell) => cell.getAttribute("data-offset"))).toEqual(["15", "16"]);
    expect(marked[0].closest("[data-testid=hex-row]")).not.toBe(marked[1].closest("[data-testid=hex-row]"));

    findFor("get /");
    expect(findCount()).toHaveTextContent("1 of 1 · as text");
    findFor("zz");
    expect(findCount()).toHaveTextContent("no match · as text");
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

describe("wrap long lines", () => {
  const wrapped = () => view().contentDOM.classList.contains("cm-lineWrapping");

  it("is on by default, flipped from ⋯ and by ⌥Z on text, JSON and XML", async () => {
    await open(payload({ text: "a long line", typeHint: "text/plain" }));
    expect(wrapped()).toBe(true);
    fireEvent.click(moreCheck(/^wrap long lines/));
    expect(wrapped()).toBe(false);
    fireEvent.keyDown(document.body, { key: "Ω", code: "KeyZ", altKey: true });
    expect(wrapped()).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    expect(screen.getByRole("menuitemcheckbox", { name: /^wrap long lines/ })).toHaveAttribute("aria-checked", "true");
  });

  it("⌥Z is left to a field being typed in", async () => {
    await open(payload({ text: "x", typeHint: "text/plain" }));
    fireEvent.keyDown(findBox(), { key: "Ω", code: "KeyZ", altKey: true });
    expect(wrapped()).toBe(true);
  });
});

describe("XML re-indent", () => {
  const xml = '<?xml version="1.0"?><a><b>two  words</b><c/></a>';
  const indented = '<?xml version="1.0"?>\n<a>\n  <b>two  words</b>\n  <c/>\n</a>';

  it("is a named step from ⋯ and from ⌥⇧F; undo walks it; restore returns the bytes", async () => {
    await open(payload({ text: xml, typeHint: "application/xml" }));
    fireEvent.click(moreItem(/^re-indent/));
    expect(doc()).toBe(indented);
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 1 edit");
    key("z");
    expect(doc()).toBe(xml);
    fireEvent.keyDown(document.body, { key: "Ï", code: "KeyF", altKey: true, shiftKey: true });
    expect(doc()).toBe(indented);
    fireEvent.click(moreItem(/^restore original/));
    expect(doc()).toBe(xml);
  });

  it("keeps the document's CRLF", async () => {
    await open(payload({ text: "<a>\r\n<b/></a>", typeHint: "application/xml" }));
    fireEvent.click(moreItem(/^re-indent/));
    fireEvent.click(screen.getByRole("button", { name: "copy" }));
    expect(writeText).toHaveBeenLastCalledWith("<a>\r\n  <b/>\r\n</a>");
  });

  it("on a text that is not well-formed, says where and does nothing by key", async () => {
    await open(payload({ text: "<a><b></a>", typeHint: "application/xml" }));
    const item = moreItem(/^re-indent/);
    expect(item).toHaveAttribute("aria-disabled", "true");
    expect(item).toHaveTextContent("not well-formed at line 1, column 7: </a> closes <b>");
    fireEvent.keyDown(document.body, { key: "Escape" });
    fireEvent.keyDown(document.body, { key: "Ï", code: "KeyF", altKey: true, shiftKey: true });
    expect(doc()).toBe("<a><b></a>");
    expect(screen.queryByTestId("modified")).toBeNull();
  });

  it("is never offered on a read-only XML text", async () => {
    await open(payload({ text: "<a>\r\n<b/>\n</a>", typeHint: "application/xml" }));
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    expect(screen.queryByRole("menuitem", { name: /^re-indent/ })).toBeNull();
    expect(screen.getByRole("group", { name: "fold to" })).toBeInTheDocument();
  });
});


describe("javascript and css", () => {
  const js = "function f(a){if(a){return a/2}return 1}\nlet total=f(4)";
  const css = "@media (min-width:1px){a{color:red}b{margin:0}}";

  async function openCode(text: string, typeHint: string) {
    await open(payload({ text, typeHint }));
    await act(async () => {
      await loadBeautifier();
    });
  }

  it("opens each under its own kind, highlighted, and find marks as on text", async () => {
    await openCode(js, "application/javascript");
    expect(screen.getByRole("button", { name: "treat the data as" })).toHaveTextContent("JavaScript");
    expect(screen.getByTestId("data-surface").querySelector(".cm-line [class]")).not.toBeNull();
    findFor("total");
    expect(findCount()).toHaveTextContent("1");
    cleanup();
    await openCode(css, "text/css");
    expect(screen.getByRole("button", { name: "treat the data as" })).toHaveTextContent("CSS");
  });

  it("reformat is a named step from ⋯ and from ⌥⇧F; undo walks it; restore returns the bytes", async () => {
    await openCode(js, "application/javascript");
    fireEvent.click(moreItem(/^reformat/));
    expect(doc().split("\n").length).toBeGreaterThan(5);
    expect(doc().replace(/\s/g, "")).toBe(js.replace(/\s/g, ""));
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 1 edit");
    key("z");
    expect(doc()).toBe(js);
    fireEvent.keyDown(document.body, { key: "Ï", code: "KeyF", altKey: true, shiftKey: true });
    expect(doc().split("\n").length).toBeGreaterThan(5);
    fireEvent.click(moreItem(/^restore original/));
    expect(doc()).toBe(js);
  });

  it("a formatter that cannot load is named on the disabled row, not called loading", async () => {
    const failing = vi.spyOn(codeTools, "loadBeautifier").mockRejectedValue(new Error("chunk failed"));
    try {
      await open(payload({ text: js, typeHint: "application/javascript" }));
      await act(async () => {});
      const item = moreItem(/^reformat/);
      expect(item).toHaveAttribute("aria-disabled", "true");
      expect(item).toHaveTextContent("the formatter could not be loaded");
    } finally {
      failing.mockRestore();
    }
  });

  it("reformats css the same way and keeps CRLF", async () => {
    await openCode("a{color:red}\r\nb{margin:0}", "text/css");
    fireEvent.click(moreItem(/^reformat/));
    expect(doc()).toContain("a {\n");
    fireEvent.click(screen.getByRole("button", { name: "copy" }));
    expect(writeText).toHaveBeenLastCalledWith(expect.stringMatching(/^a \{\r\n {2}color: red\r\n\}/));
  });

  it("is never offered on a read-only text", async () => {
    await openCode("a=1;\r\nb=2;\n", "application/javascript");
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    expect(screen.queryByRole("menuitem", { name: /^reformat/ })).toBeNull();
    expect(screen.getByRole("group", { name: "fold to" })).toBeInTheDocument();
  });

  it("a text under a javascript label that is only whitespace has nothing to reformat", async () => {
    await openCode("  \n ", "application/javascript");
    const item = moreItem(/^reformat/);
    expect(item).toHaveAttribute("aria-disabled", "true");
    expect(item).toHaveTextContent("there is nothing to reformat");
  });

  it("opens and reformats a 1 MiB minified bundle", async () => {
    const bundle = "function f(a){return a+1}".repeat(40_000);
    await openCode(bundle, "application/javascript");
    fireEvent.click(moreItem(/^reformat/));
    expect(view().state.doc.length).toBeGreaterThan(bundle.length);
  });
});

describe("html source and preview", () => {
  const page =
    '<!doctype html><html><head><meta http-equiv="refresh" content="0;url=https://x.test"><style>p{color:red}</style></head><body><p>hello</p><script>fetch("https://x.test/b")</script><img src="https://x.test/p.gif"></body></html>';

  async function openPage(text = page, typeHint = "text/html") {
    await open(payload({ text, typeHint }));
    await act(async () => {
      await loadBeautifier();
    });
  }

  const frame = () => screen.getByTestId("data-preview") as HTMLIFrameElement;
  const showing = (name: "source" | "preview") => screen.getByRole("button", { name }).getAttribute("aria-pressed") === "true";

  it("opens as HTML in its own highlighted surface, and find marks as on text", async () => {
    await openPage();
    expect(screen.getByRole("button", { name: "treat the data as" })).toHaveTextContent("HTML");
    expect(screen.getByTestId("data-surface").querySelector(".cm-line [class]")).not.toBeNull();
    findFor("hello");
    expect(findCount()).toHaveTextContent("1");
    expect(showing("source")).toBe(true);
    expect(screen.queryByTestId("data-preview")).toBeNull();
  });

  it("re-indent is a named step from ⋯ and from ⌥⇧F; undo and restore return the bytes", async () => {
    await openPage();
    fireEvent.click(moreItem(/^re-indent/));
    expect(doc().split("\n").length).toBeGreaterThan(8);
    expect(doc().replace(/\s/g, "")).toBe(page.replace(/\s/g, ""));
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 1 edit");
    key("z");
    expect(doc()).toBe(page);
    fireEvent.keyDown(document.body, { key: "Ï", code: "KeyF", altKey: true, shiftKey: true });
    expect(doc().split("\n").length).toBeGreaterThan(8);
    fireEvent.click(moreItem(/^restore original/));
    expect(doc()).toBe(page);
  });

  it("is never offered on a read-only text", async () => {
    await openPage("<p>a</p>\r\n<p>b</p>\n");
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    expect(screen.queryByRole("menuitem", { name: /^re-indent/ })).toBeNull();
  });

  it("the preview is a frame with an empty sandbox over the working copy, and the page's scripts and remote loads are not in play", async () => {
    await openPage();
    fireEvent.click(screen.getByRole("button", { name: "preview" }));
    expect(frame().getAttribute("sandbox")).toBe("");
    const built = frame().getAttribute("srcdoc")!;
    expect(built).toContain("<p>hello</p>");
    expect(built).toContain("<style>p{color:red}</style>");
    expect(built.indexOf("Content-Security-Policy")).toBeLessThan(built.indexOf("<style>p{color:red}"));
    expect(built).toContain("default-src 'none'");
    expect(built.toLowerCase()).not.toContain("refresh");
    expect(screen.getByText("scripts, forms and anything remote are not run or loaded")).toBeInTheDocument();
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("the preview follows the working copy: an edit made in source shows on return", async () => {
    await openPage("<p>hello</p>");
    typeInto(0, "<h1>new</h1>");
    fireEvent.click(screen.getByRole("button", { name: "preview" }));
    expect(frame().getAttribute("srcdoc")).toContain("<h1>new</h1>");
    fireEvent.click(screen.getByRole("button", { name: "source" }));
    expect(doc()).toBe("<h1>new</h1><p>hello</p>");
  });

  it("the preview is read-only: no editor, no find bar, re-indent disabled with its reason", async () => {
    await openPage();
    fireEvent.click(screen.getByRole("button", { name: "preview" }));
    expect(screen.queryByTestId("data-surface")).toBeNull();
    expect(screen.queryByRole("textbox", { name: /find/i })).toBeNull();
    const item = moreItem(/^re-indent/);
    expect(item).toHaveAttribute("aria-disabled", "true");
    expect(item).toHaveTextContent("shown in preview");
  });

  it("⇧⌘V, the ⋯ choice and the switch agree", async () => {
    await openPage();
    key("v", { shiftKey: true });
    expect(showing("preview")).toBe(true);
    key("v", { shiftKey: true });
    expect(showing("source")).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    fireEvent.click(within(screen.getByRole("menu", { name: "more" })).getByRole("menuitemradio", { name: "preview" }));
    expect(showing("preview")).toBe(true);
    expect(frame()).toBeInTheDocument();
  });

  it("⌘F leaves the preview for the source and focuses find", async () => {
    await openPage();
    fireEvent.click(screen.getByRole("button", { name: "preview" }));
    key("f");
    expect(showing("source")).toBe(true);
    expect(screen.getByTestId("data-surface")).toBeInTheDocument();
    expect(document.activeElement).toBe(findBox());
  });

  it("coming back to html after another kind opens the source, not the preview", async () => {
    await openPage();
    fireEvent.click(screen.getByRole("button", { name: "preview" }));
    pickKind("text");
    pickKind("HTML");
    expect(showing("source")).toBe(true);
    expect(screen.getByTestId("data-surface")).toBeInTheDocument();
  });

  it("⇧⌘V typed in the find box is left to the box", async () => {
    await openPage();
    fireEvent.keyDown(findBox(), { key: "v", metaKey: true, shiftKey: true });
    expect(showing("source")).toBe(true);
  });

  it("choosing another kind in the picker leaves the preview; other kinds have no switch", async () => {
    await openPage();
    fireEvent.click(screen.getByRole("button", { name: "preview" }));
    pickKind("text");
    expect(screen.queryByTestId("data-preview")).toBeNull();
    expect(screen.queryByRole("button", { name: "preview" })).toBeNull();
    key("v", { shiftKey: true });
    expect(screen.queryByTestId("data-preview")).toBeNull();
  });
});

describe("decode", () => {
  const b64url = (text: string) => btoa(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  const TOKEN = `${b64url('{"alg":"RS256"}')}.${b64url('{"sub":"8812"}')}.c2ln`;
  const JSON_DOC = `{\n  "session_token": "${TOKEN}",\n  "plan": "team"\n}`;
  const GZIP_B64 = btoa(String.fromCharCode(0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03));
  const FORM = `grant=jwt&assertion=${TOKEN}&note=50%2525%2520off`;

  function select(text: string) {
    const from = doc().indexOf(text);
    act(() => {
      view().dispatch({ selection: { anchor: from, head: from + text.length } });
    });
  }

  function decodeFromMenu(target: HTMLElement, item: RegExp) {
    fireEvent.contextMenu(target, { clientX: 10, clientY: 10 });
    fireEvent.click(within(screen.getByRole("menu", { name: "the selection" })).getByRole("menuitem", { name: item }));
  }

  const peek = () => screen.getByTestId("decode-peek");
  const decodedText = (at = 0) =>
    EditorView.findFromDOM(within(peek()).getAllByTestId("decoded")[at].querySelector(".cm-editor") as HTMLElement)!.state.doc.toString();

  async function openWith(handed: DataPayload, opened = vi.fn(() => Promise.resolve())) {
    render(<DataWindowScreen take={() => Promise.resolve(handed)} open={opened} />);
    await screen.findByRole("heading", { name: handed.title });
    return opened;
  }

  function rightClickAt(offset: number) {
    vi.spyOn(view(), "posAtCoords").mockReturnValue(offset);
    fireEvent.contextMenu(screen.getByTestId("data-surface"), { clientX: 10, clientY: 10 });
  }

  const selected = () => {
    const { from, to } = view().state.selection.main;
    return doc().slice(from, to);
  };

  it("waits for a value under the pointer or a selection, then reads a JWT under its line, leaving the working copy as it was", async () => {
    await open(payload({ text: JSON_DOC }));
    rightClickAt(0);
    expect(screen.getByRole("menuitem", { name: /^decode/ })).toHaveTextContent("no value here: right-click on one or select it");
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

    select(TOKEN);
    await waitFor(() => expect(footer()).toHaveTextContent(`selection ${TOKEN.length} characters`));
    decodeFromMenu(screen.getByTestId("data-surface"), /^decode/);

    expect(within(peek()).getByText("decoded as")).toBeInTheDocument();
    expect(within(peek()).getByText("JWT")).toBeInTheDocument();
    expect(within(peek()).queryByRole("radiogroup")).toBeNull();
    expect(decodedText(0)).toBe('{\n  "alg": "RS256"\n}');
    expect(decodedText(1)).toBe('{\n  "sub": "8812"\n}');
    expect(within(peek()).getByText("signature · not verified")).toBeInTheDocument();
    expect(within(peek()).getByTestId("signature")).toHaveTextContent("c2ln");
    const above = peek().parentElement!.previousElementSibling!;
    expect(above).toHaveClass("cm-line");
    expect(above).toHaveTextContent('"session_token"');
    expect(screen.queryByTestId("modified")).toBeNull();
  });

  it("right-clicking a value with nothing selected decodes that value and selects it", async () => {
    await open(payload({ text: JSON_DOC }));
    rightClickAt(doc().indexOf(TOKEN) + 12);
    fireEvent.click(screen.getByRole("menuitem", { name: /^decode/ }));
    expect(within(peek()).getByText("JWT")).toBeInTheDocument();
    expect(selected()).toBe(TOKEN);
  });

  it("open all's headers open as text, and a right-click on a bearer token there reads it as a JWT", async () => {
    const flow: FlowView = {
      id: "f9",
      started_at: "2026-11-05T10:00:00Z",
      duration_ms: 1,
      method: "GET",
      uri: "https://api.example.com/me",
      status: 200,
      source: "captured",
      seed: null,
      request_http_version: "HTTP/1.1",
      response_http_version: "HTTP/1.1",
      request_headers: [
        { name: "Accept", value: "application/json" },
        { name: "Authorization", value: `Bearer ${TOKEN}` },
        { name: "Cookie", value: `session=${"x".repeat(4096)}` },
      ],
      response_headers: [],
      request_body: { state: "empty" },
      response_body: { state: "empty" },
    };
    await open(headersPayload(flow, "request"));
    expect(doc()).toBe(`Accept: application/json\nAuthorization: Bearer ${TOKEN}\nCookie: session=${"x".repeat(4096)}`);

    rightClickAt(doc().indexOf(TOKEN) + 12);
    fireEvent.click(screen.getByRole("menuitem", { name: /^decode/ }));

    expect(within(peek()).getByText("JWT")).toBeInTheDocument();
    expect(decodedText(1)).toBe('{\n  "sub": "8812"\n}');
    expect(selected()).toBe(TOKEN);
  });

  it("⌘⇧D takes the value out of a rough selection, its key and quotes dropped", async () => {
    await open(payload({ text: JSON_DOC }));
    select(`"session_token": "${TOKEN}",`);
    key("D", { shiftKey: true });
    expect(within(peek()).getByText("JWT")).toBeInTheDocument();
    expect(selected()).toBe(TOKEN);
    fireEvent.click(within(peek()).getByRole("button", { name: "replace with the payload" }));
    expect(doc()).toBe(`{\n  "session_token": "{\\"sub\\":\\"8812\\"}",\n  "plan": "team"\n}`);
  });

  it("valid base64 with no known format reads as binary to open, not as unreadable", async () => {
    const proto = btoa(String.fromCharCode(0x08, 0x96, 0x01, 0x12, 0x0b, ...new TextEncoder().encode("suggestions"), 0x18, 0xfe, 0x03));
    await open(payload({ text: `{"t": "${proto}"}` }));
    select(proto);
    key("D", { shiftKey: true });
    expect(peek()).toHaveTextContent("19 B of binary · no known signature");
    expect(within(peek()).getByRole("button", { name: "open in data window" })).toBeInTheDocument();
  });

  it("⇄ replaces the token with its payload as JSON string content, one step undone by ⌘Z", async () => {
    await open(payload({ text: JSON_DOC }));
    select(TOKEN);
    key("D", { shiftKey: true });
    fireEvent.click(within(peek()).getByRole("button", { name: "replace with the payload" }));
    expect(doc()).toBe(`{\n  "session_token": "{\\"sub\\":\\"8812\\"}",\n  "plan": "team"\n}`);
    expect(screen.queryByTestId("decode-peek")).toBeNull();
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 1 edit");
    key("z");
    expect(doc()).toBe(JSON_DOC);
  });

  it("replaces only the selected occurrence, its surrounding spaces kept, in the text's own breaks", async () => {
    const text = "a%20b\r\n  a%20b  \r\nend";
    await open(payload({ text, typeHint: "text/plain" }));
    act(() => {
      view().dispatch({ selection: { anchor: 6, head: 15 } });
    });
    key("D", { shiftKey: true });
    expect(within(peek()).getByText("URL")).toBeInTheDocument();
    expect(decodedText()).toBe("a b");
    fireEvent.click(within(peek()).getByRole("button", { name: "replace with decoded" }));
    expect(doc()).toBe("a%20b\n  a b  \nend");
    fireEvent.click(screen.getByRole("button", { name: "copy" }));
    expect(writeText).toHaveBeenLastCalledWith("a%20b\r\n  a b  \r\nend");
  });

  it("⌘⇧D after a JSON escape decodes the value behind it and ⇄ keeps the escape, the document valid", async () => {
    const value = btoa("hello");
    const text = `{"a": "\\n${value}"}`;
    await open(payload({ text }));
    act(() => {
      view().dispatch({ selection: { anchor: text.indexOf(value) + 3 } });
    });
    key("D", { shiftKey: true });
    expect(selected()).toBe(value);
    expect(decodedText()).toBe("hello");
    fireEvent.click(within(peek()).getByRole("button", { name: "replace with decoded" }));
    expect(doc()).toBe(`{"a": "\\nhello"}`);
    expect(JSON.parse(doc())).toEqual({ a: "\nhello" });
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 1 edit");
  });

  it("a malformed token says why, offers no act on it, and changes nothing", async () => {
    const bad = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiI4ODEyIn0";
    await open(payload({ text: `{"t": "${bad}"}` }));
    select(bad);
    key("D", { shiftKey: true });
    expect(within(peek()).getByText("decode")).toBeInTheDocument();
    expect(peek()).toHaveTextContent(
      "not base64, URL-encoded, a JWT or a time: 2 parts separated by dots. A JWT has 3: header, payload, signature",
    );
    expect(peek()).toHaveTextContent("the selection is left as it was");
    expect(within(peek()).queryByRole("button", { name: /replace/ })).toBeNull();
    expect(within(peek()).queryByRole("button", { name: "open in data window" })).toBeNull();
    expect(screen.queryByTestId("modified")).toBeNull();
  });

  it("binary offers ↗ only, which opens the decoded bytes as data of their own", async () => {
    const opened = await openWith(payload({ text: `{"blob": "${GZIP_B64}"}` }));
    select(GZIP_B64);
    key("D", { shiftKey: true });
    expect(peek()).toHaveTextContent("10 B of binary · looks like gzip");
    expect(within(peek()).queryByRole("button", { name: /replace/ })).toBeNull();
    fireEvent.click(within(peek()).getByRole("button", { name: "open in data window" }));
    await waitFor(() => expect(opened).toHaveBeenCalledTimes(1));
    const handed = (opened.mock.calls[0] as unknown as [DataPayload])[0];
    expect(handed.title).toBe("response body · GET api.example.com/v2/x · decoded base64");
    expect(Array.from(handed.bytes)).toEqual([0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03]);
    expect(handed.typeHint).toBeUndefined();
    expect(handed.key).toMatch(/^decoded-[0-9a-f]{32}$/);
    expect(handed.key).not.toBe("k");
  });

  it("a JWT opens its payload as JSON", async () => {
    const opened = await openWith(payload({ text: JSON_DOC }));
    select(TOKEN);
    key("D", { shiftKey: true });
    fireEvent.click(within(peek()).getByRole("button", { name: "open in data window" }));
    await waitFor(() => expect(opened).toHaveBeenCalledTimes(1));
    const handed = (opened.mock.calls[0] as unknown as [DataPayload])[0];
    expect(new TextDecoder().decode(handed.bytes)).toBe('{"sub":"8812"}');
    expect(handed.typeHint).toBe("application/json");
    expect(handed.title).toMatch(/ · decoded JWT$/);
  });

  it("a form row decodes its value from the right-click or ⌘⇧D and ⇄ re-encodes the payload into it", async () => {
    await open(payload({ text: FORM, typeHint: "application/x-www-form-urlencoded" }));
    decodeFromMenu(screen.getAllByTestId("form-pair")[1], /^decode the value/);
    expect(within(peek()).getByText("JWT")).toBeInTheDocument();
    expect(peek()).toHaveTextContent("decoded as");
    expect(screen.getAllByTestId("form-pair")[1].nextElementSibling).toContainElement(peek());

    fireEvent.click(screen.getByRole("button", { name: "close the peek" }));
    expect(screen.queryByTestId("decode-peek")).toBeNull();
    fireEvent.click(screen.getAllByTestId("form-pair")[2]);
    key("D", { shiftKey: true });
    expect(within(peek()).getByText("URL")).toBeInTheDocument();
    expect(decodedText()).toBe("50% off");
    fireEvent.click(screen.getAllByTestId("form-pair")[1]);
    key("D", { shiftKey: true });
    fireEvent.click(within(peek()).getByRole("button", { name: "replace with the payload" }));
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 1 edit");
    expect(within(screen.getAllByTestId("form-pair")[1]).getByText('{"sub":"8812"}')).toBeInTheDocument();
    fireEvent.click(moreItem(/^undo/));
    expect(within(screen.getAllByTestId("form-pair")[1]).getByText(TOKEN)).toBeInTheDocument();
  });

  it("an empty value has nothing to decode", async () => {
    await open(payload({ text: "a=&b=1", typeHint: "application/x-www-form-urlencoded" }));
    fireEvent.contextMenu(screen.getAllByTestId("form-pair")[0], { clientX: 5, clientY: 5 });
    expect(screen.getByRole("menuitem", { name: /^decode the value/ })).toHaveTextContent("empty value");
  });

  it("a JSON number decodes as time: local, UTC and what it was read as; ⇄ quotes the local time, one step undone", async () => {
    const local = (time("1700000000") as TimeReading).local;
    await open(payload({ text: '{"exp": 1700000000, "n": 1}' }));
    rightClickAt(doc().indexOf("1700000000") + 4);
    fireEvent.click(screen.getByRole("menuitem", { name: /^decode/ }));
    expect(selected()).toBe("1700000000");
    expect(within(peek()).getByText("time")).toBeInTheDocument();
    expect(within(peek()).getByText(`local · ${systemZone()}`)).toBeInTheDocument();
    expect(within(peek()).getByTestId("local-time")).toHaveTextContent(local);
    expect(within(peek()).getByTestId("utc-time")).toHaveTextContent("2023-11-14 22:13:20Z");
    expect(within(peek()).getByTestId("read-as")).toHaveTextContent(/^Unix time, seconds · \d+ years ago$/);
    fireEvent.click(within(peek()).getByRole("button", { name: "replace with local time" }));
    expect(doc()).toBe(`{"exp": "${local}", "n": 1}`);
    expect(screen.getByTestId("modified")).toHaveTextContent("modified · 1 edit");
    key("z");
    expect(doc()).toBe('{"exp": 1700000000, "n": 1}');
  });

  it("a date-time string is found whole from the pointer and replaced inside its quotes", async () => {
    const at = "2023-11-14T22:13:20Z";
    await open(payload({ text: `{"at": "${at}"}` }));
    rightClickAt(doc().indexOf(at) + 12);
    fireEvent.click(screen.getByRole("menuitem", { name: /^decode/ }));
    expect(selected()).toBe(at);
    expect(within(peek()).getByTestId("read-as")).toHaveTextContent(/^ISO 8601 · /);
    fireEvent.click(within(peek()).getByRole("button", { name: "replace with local time" }));
    expect(doc()).toBe(`{"at": "${(time(at) as TimeReading).local}"}`);
  });

  it("a form row's HTTP date decodes by ⌘⇧D and ⇄ writes the local time into the value", async () => {
    const expires = "Tue, 14 Nov 2023 22:13:20 GMT";
    await open(payload({ text: `id=1&expires=${encodeURIComponent(expires)}`, typeHint: "application/x-www-form-urlencoded" }));
    fireEvent.click(screen.getAllByTestId("form-pair")[1]);
    key("D", { shiftKey: true });
    expect(within(peek()).getByTestId("read-as")).toHaveTextContent(/^HTTP date · /);
    expect(within(peek()).getByTestId("utc-time")).toHaveTextContent("2023-11-14 22:13:20Z");
    fireEvent.click(within(peek()).getByRole("button", { name: "replace with local time" }));
    expect(within(screen.getAllByTestId("form-pair")[1]).getByText((time(expires) as TimeReading).local)).toBeInTheDocument();
  });

  it("a date-time with no zone reads as UTC and says so", async () => {
    await open(payload({ text: '{"lastModifiedOn": "2023-11-14T22:13:20",' }));
    rightClickAt(doc().indexOf("2023") + 2);
    fireEvent.click(screen.getByRole("menuitem", { name: /^decode/ }));
    expect(selected()).toBe("2023-11-14T22:13:20");
    expect(within(peek()).getByTestId("utc-time")).toHaveTextContent("2023-11-14 22:13:20Z");
    expect(within(peek()).getByTestId("read-as")).toHaveTextContent(/^ISO 8601, no zone: read as UTC · /);
  });

  it("a date reads as its calendar day, with no local or UTC line", async () => {
    await open(payload({ text: '{"homeDeploymentStartDate": "2026-01-14",' }));
    rightClickAt(doc().indexOf("2026") + 2);
    fireEvent.click(screen.getByRole("menuitem", { name: /^decode/ }));
    expect(selected()).toBe("2026-01-14");
    expect(within(peek()).getByText("date")).toBeInTheDocument();
    expect(within(peek()).getByTestId("local-time")).toHaveTextContent("2026-01-14, Wednesday");
    expect(within(peek()).queryByTestId("utc-time")).toBeNull();
    expect(within(peek()).getByTestId("read-as")).toHaveTextContent(/^ISO 8601 date · /);
  });

  it("a date off the calendar says why and changes nothing", async () => {
    await open(payload({ text: '{"at": "2023-02-30T22:13:20Z"}' }));
    rightClickAt(doc().indexOf("2023") + 2);
    fireEvent.click(screen.getByRole("menuitem", { name: /^decode/ }));
    expect(selected()).toBe("2023-02-30T22:13:20Z");
    expect(peek()).toHaveTextContent("not base64, URL-encoded, a JWT or a time: 2023-02-30 is not a day of the calendar");
    expect(within(peek()).queryByRole("button", { name: /replace/ })).toBeNull();
    expect(screen.queryByTestId("modified")).toBeNull();
  });

  it("Esc, an edit or another kind closes the peek", async () => {
    await open(payload({ text: JSON_DOC }));
    select(TOKEN);
    key("D", { shiftKey: true });
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.queryByTestId("decode-peek")).toBeNull();

    select(TOKEN);
    key("D", { shiftKey: true });
    typeInto(0, " ");
    expect(screen.queryByTestId("decode-peek")).toBeNull();

    select(TOKEN);
    key("D", { shiftKey: true });
    expect(peek()).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "treat the data as" }));
    fireEvent.click(screen.getByRole("option", { name: "text" }));
    expect(screen.queryByTestId("decode-peek")).toBeNull();
  });
});
