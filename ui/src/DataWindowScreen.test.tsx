import { afterEach, beforeAll, beforeEach, describe, expect, it, vi, type MockInstance } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { EditorView } from "@codemirror/view";
import * as files from "./body";
import { DataWindowScreen } from "./DataWindowScreen";
import type { DataPayload } from "./dataWindow";

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

describe("the header reads the caption in parts", () => {
  it("what above; the method, host, path and query on one line, the whole caption on hover", async () => {
    const title = "request body · raw · POST www.example.com/ajax/bnzai?__a=1&__req=7n&lsd=x";
    await open(payload({ title }));
    const heading = screen.getByRole("heading", { name: title });
    expect(heading).toHaveAttribute("title", title);
    expect(screen.getByText("request body · raw").tagName).toBe("P");
    expect(within(heading).getByText("POST")).toBeInTheDocument();
    expect(within(heading).getByText("www.example.com")).toHaveClass("text-ink");
    expect(within(heading).getByText("/ajax/bnzai")).toHaveClass("text-ink-body");
    expect(within(heading).getByText("?__a=1&__req=7n&lsd=x")).toHaveClass("text-ink-mute");
    expect(within(heading).getByText("3 params")).toBeInTheDocument();
    expect(within(heading).getByText("?__a=1&__req=7n&lsd=x").parentElement).toHaveClass("truncate");
  });

  it("a target with no query names no params", async () => {
    await open(payload());
    expect(within(screen.getByRole("heading")).queryByText(/param/)).toBeNull();
  });

  it("a caption of another shape is shown whole on one line", async () => {
    await open(payload({ title: "a pasted note" }));
    expect(screen.getByRole("heading", { name: "a pasted note" })).toHaveClass("truncate");
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
    expect(screen.getByTestId("detected")).toHaveTextContent("detected from application/json");
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
    expect(screen.getByTestId("detected")).toHaveTextContent("detected from its bytes");
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
    expect(screen.getByTestId("detected")).toHaveTextContent("your choice · detected JSON from application/json");
    fireEvent.click(screen.getByRole("button", { name: "treat the data as" }));
    fireEvent.click(screen.getByRole("option", { name: "JSON" }));
    expect(screen.getByTestId("detected")).toHaveTextContent(/^detected from application\/json$/);
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
    expect(screen.getByRole("menuitem", { name: /^copy/ })).toHaveTextContent("select some text");
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

    act(() => {
      view().dispatch({ selection: { anchor: 5, head: 9 } });
    });
    await waitFor(() => expect(footer()).toHaveTextContent("selection 4 characters"));
    fireEvent.contextMenu(screen.getByTestId("data-surface"), { clientX: 10, clientY: 10 });
    fireEvent.click(screen.getByRole("menuitem", { name: /^copy/ }));
    expect(writeText).toHaveBeenLastCalledWith("plan");
  });
});
