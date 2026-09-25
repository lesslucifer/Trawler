import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { BodyViewer } from "./BodyViewer";
import { hexDump, HEX_MAX, TEXT_MAX, TREE_MAX } from "./body";
import { CONFIRM_MS } from "./components/index";
import type { BodyFetch, BodyView, FlowView, TrawlerApi } from "./api";
import { apiStub } from "./apiStub";

function flow(over: Partial<FlowView> = {}): FlowView {
  return {
    id: "0192a1b2-c3d4-7e8f-9012-3456789abcde",
    started_at: "2026-07-13T10:00:00Z",
    duration_ms: 5,
    method: "GET",
    uri: "http://example.com/",
    status: 200,
    source: "captured",
    seed: null,
    request_http_version: "HTTP/1.1",
    response_http_version: "HTTP/1.1",
    request_headers: [],
    response_headers: [],
    request_body: { state: "empty" },
    response_body: { state: "captured", size: 1, data_base64: "" },
    ...over,
  };
}

// Viewer vs the mock API: the returned bytes/content-type drive the dispatch,
// exactly like GET /flows/{id}/{leg}/body would.
function apiWith(body: Partial<BodyFetch>): TrawlerApi {
  return apiStub({
    getBody: vi.fn(async () => ({
      bytes: new Uint8Array(),
      contentType: null,
      ...body,
    })),
  });
}

const text = (s: string) => new TextEncoder().encode(s);

afterEach(() => vi.unstubAllGlobals());

test("the leg is a prop the pane never asks about", async () => {
  const api = apiWith({ bytes: text("req"), contentType: "text/plain" });
  render(
    <BodyViewer
      api={api}
      flow={flow({ request_body: { state: "captured", size: 3, data_base64: "" } })}
      leg="request"
    />,
  );

  await screen.findByTestId("body-text");
  expect(api.getBody).toHaveBeenCalledWith(flow().id, "request", true);
  expect(screen.queryByRole("button", { name: "request" })).toBeNull();
  expect(screen.queryByRole("button", { name: "response" })).toBeNull();
});

test("a JSON body renders through JsonRenderer's source surface, not a lossy tree", async () => {
  const api = apiWith({
    bytes: text('{\n  "user": "ada"\n}'),
    contentType: "application/json",
  });
  const { container } = render(<BodyViewer api={api} flow={flow()} leg="response" />);

  await screen.findByTestId("body-json");
  const shown = container.querySelector(".cm-content")?.textContent ?? "";
  expect(() => JSON.parse(shown)).not.toThrow();
  expect(container.querySelector("details")).toBeNull();
});

test("an XML body dispatches to XmlRenderer instead of falling through to text", async () => {
  const api = apiWith({
    bytes: text("<order>\n  <item qty=\"2\"/>\n</order>"),
    contentType: "application/xml",
  });
  render(<BodyViewer api={api} flow={flow()} leg="response" />);

  expect(await screen.findByTestId("body-xml")).toBeInTheDocument();
  expect(screen.queryByTestId("body-text")).toBeNull();
});

test("a text body renders through TextRenderer", async () => {
  const api = apiWith({ bytes: text("plain words"), contentType: "text/plain" });
  render(<BodyViewer api={api} flow={flow()} leg="response" />);
  expect(await screen.findByTestId("body-text")).toHaveTextContent("plain words");
});

test("a binary body renders through HexRenderer's toned dump", async () => {
  const api = apiWith({ bytes: new Uint8Array([0, 1, 2]), contentType: "application/octet-stream" });
  render(<BodyViewer api={api} flow={flow()} leg="response" />);

  const hex = await screen.findByTestId("body-hex");
  expect(hex).toHaveTextContent("00000000");
  expect(hex).toHaveTextContent("00 01 02");
});

test("a form body renders through FormRenderer, each value its own copy target", async () => {
  const api = apiWith({
    bytes: text("user=ada&mode=fast%20one"),
    contentType: "application/x-www-form-urlencoded",
  });
  render(<BodyViewer api={api} flow={flow()} leg="response" />);

  const form = await screen.findByTestId("body-form");
  expect(form).toHaveTextContent("user");
  expect(form).toHaveTextContent("fast one");
  expect(screen.getByRole("button", { name: "copy user" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "copy mode" })).toBeInTheDocument();
});

test("an image body renders through ImageRenderer over an object URL", async () => {
  vi.stubGlobal("URL", {
    ...URL,
    createObjectURL: vi.fn(() => "blob:fake"),
    revokeObjectURL: vi.fn(),
  });
  const api = apiWith({ bytes: new Uint8Array([137, 80]), contentType: "image/png" });
  const { unmount } = render(<BodyViewer api={api} flow={flow()} leg="response" />);

  expect(await screen.findByTestId("body-image")).toHaveAttribute("src", "blob:fake");
  unmount();
});

// Both positions are decoded — they differ in re-indenting — so "decoded"
// labelled what the two share. The absence assertion is the one that catches a
// half-done rename.
test("the switch names the two views the lexicon settled on, never 'decoded'", async () => {
  const api = apiWith({ bytes: text('{"a":1}'), contentType: "application/json" });
  render(<BodyViewer api={api} flow={flow()} leg="response" />);
  await screen.findByTestId("body-json");

  expect(screen.getByRole("radio", { name: "pretty" })).toBeChecked();
  expect(screen.getByRole("radio", { name: "raw" })).not.toBeChecked();
  expect(screen.queryByRole("radio", { name: "decoded" })).toBeNull();
});

test("the raw segment refetches undecoded and dispatches on the bytes, not the type", async () => {
  const api = apiWith({ bytes: text('{"a":1}'), contentType: "application/json" });
  render(<BodyViewer api={api} flow={flow()} leg="response" />);
  await screen.findByTestId("body-json");

  fireEvent.click(screen.getByRole("radio", { name: "raw" }));

  await screen.findByTestId("body-text");
  expect(api.getBody).toHaveBeenLastCalledWith(flow().id, "response", false);

  fireEvent.click(screen.getByRole("radio", { name: "pretty" }));

  await screen.findByTestId("body-json");
  expect(api.getBody).toHaveBeenLastCalledWith(flow().id, "response", true);
});

test("a text body over TEXT_MAX states its truncation instead of swallowing it", async () => {
  const big = "x".repeat(TEXT_MAX + 1024);
  const api = apiWith({ bytes: text(big), contentType: "text/plain" });
  render(<BodyViewer api={api} flow={flow()} leg="response" />);

  await screen.findByTestId("body-text");
  const marker = screen.getByText(/⚠ showing .* of .* — save to see all/);
  expect(marker).toBeInTheDocument();
  const [, shown, total] = /showing (\S+ \S+) of (\S+ \S+)/.exec(marker.textContent ?? "") ?? [];
  expect(shown).not.toBe(total);
});

test("a hex body over HEX_MAX states its truncation, not an easy-to-miss ellipsis", async () => {
  const bytes = new Uint8Array(HEX_MAX + 64).fill(0xff);
  const api = apiWith({ bytes, contentType: "application/octet-stream" });
  render(<BodyViewer api={api} flow={flow()} leg="response" />);

  await screen.findByTestId("body-hex");
  expect(screen.getByText(/⚠ showing .* of .* — save to see all/)).toBeInTheDocument();
  expect(screen.queryByText(/more bytes/)).toBeNull();
});

test("a JSON body over the tree cap falls back to text with the family's marker", async () => {
  const huge = `{"pad":"${"x".repeat(TREE_MAX)}"}`;
  const api = apiWith({ bytes: text(huge), contentType: "application/json" });
  render(<BodyViewer api={api} flow={flow()} leg="response" />);

  expect(await screen.findByTestId("body-text")).toBeInTheDocument();
  expect(screen.queryByTestId("body-json")).toBeNull();
  expect(screen.getByText(/⚠ showing .* of .* — save to see all/)).toBeInTheDocument();
});

test("copy body yields valid JSON — the payload is the bytes, not a rendered form", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  const source = '{\n  "user": "ada",\n  "tags": [1, 2]\n}';
  const api = apiWith({ bytes: text(source), contentType: "application/json" });
  render(<BodyViewer api={api} flow={flow()} leg="response" />);
  await screen.findByTestId("body-json");

  fireEvent.click(screen.getByRole("button", { name: /copy body/ }));

  expect(writeText).toHaveBeenCalledWith(source);
  expect(() => JSON.parse(writeText.mock.calls[0][0])).not.toThrow();
});

test("copy body on a hex-rendered body copies the dump the pane shows", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  const bytes = new Uint8Array([0, 1, 2]);
  const api = apiWith({ bytes, contentType: "application/octet-stream" });
  render(<BodyViewer api={api} flow={flow()} leg="response" />);
  await screen.findByTestId("body-hex");

  fireEvent.click(screen.getByRole("button", { name: /copy body/ }));

  expect(writeText).toHaveBeenCalledWith(hexDump(bytes));
});

test("the copy confirm reverts — the shipped label never did", async () => {
  vi.useFakeTimers();
  try {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    const api = apiWith({ bytes: text("payload"), contentType: "text/plain" });
    render(<BodyViewer api={api} flow={flow()} leg="response" />);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    fireEvent.click(screen.getByRole("button", { name: /copy body/ }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(screen.getByRole("button", { name: /copied/ })).toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(CONFIRM_MS + 1);
    });
    expect(screen.getByRole("button", { name: /copy body/ })).toBeInTheDocument();
  } finally {
    vi.useRealTimers();
  }
});

// CopyButton's own reset key cannot fire here: switching mode nulls `fetched`,
// which unmounts the button with the whole action group. This asserts the
// invariant that arrangement gives us, so a later refactor that keeps the
// button mounted across a refetch has to answer for a stale confirm.
test("switching mode clears a standing confirm — no 'copied' left over another body", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  const api = apiWith({ bytes: text("payload"), contentType: "text/plain" });
  render(<BodyViewer api={api} flow={flow()} leg="response" />);
  await screen.findByTestId("body-text");

  fireEvent.click(screen.getByRole("button", { name: /copy body/ }));
  await waitFor(() => expect(screen.getByRole("button", { name: /copied/ })).toBeInTheDocument());

  fireEvent.click(screen.getByRole("radio", { name: "raw" }));

  await waitFor(() => expect(screen.getByRole("button", { name: /copy body/ })).toBeInTheDocument());
  expect(screen.queryByRole("button", { name: /copied/ })).toBeNull();
});

test("an image body offers save but no copy — an image has no clipboard text form", async () => {
  vi.stubGlobal("URL", {
    ...URL,
    createObjectURL: vi.fn(() => "blob:fake"),
    revokeObjectURL: vi.fn(),
  });
  const api = apiWith({ bytes: new Uint8Array([137, 80]), contentType: "image/png" });
  const { unmount } = render(<BodyViewer api={api} flow={flow()} leg="response" />);
  await screen.findByTestId("body-image");

  expect(screen.queryByRole("button", { name: /copy body/ })).toBeNull();
  expect(screen.getByRole("button", { name: "save" })).toBeInTheDocument();
  unmount();
});

test("save writes all the bytes, which is what makes the cut marker's advice true", async () => {
  const createObjectURL = vi.fn(() => "blob:save");
  vi.stubGlobal("URL", { ...URL, createObjectURL, revokeObjectURL: vi.fn() });
  const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  const bytes = text("x".repeat(TEXT_MAX + 1024));
  const api = apiWith({ bytes, contentType: "text/plain" });
  render(<BodyViewer api={api} flow={flow()} leg="response" />);
  await screen.findByTestId("body-text");

  fireEvent.click(screen.getByRole("button", { name: "save" }));

  await waitFor(() => expect(click).toHaveBeenCalled());
  expect(createObjectURL).toHaveBeenCalledWith(expect.objectContaining({ size: bytes.length }));
  click.mockRestore();
});

// The saved file lands in a Downloads folder, where the flow id names nothing
// and a missing extension makes the OS refuse to open it.
test("save names the file after the endpoint and the served type", async () => {
  vi.stubGlobal("URL", { ...URL, createObjectURL: vi.fn(() => "blob:save"), revokeObjectURL: vi.fn() });
  const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  const downloads: string[] = [];
  const anchor = vi
    .spyOn(HTMLAnchorElement.prototype, "download", "set")
    .mockImplementation(function (this: HTMLAnchorElement, name: string) {
      downloads.push(name);
    });
  const api = apiWith({ bytes: text('{"a":1}'), contentType: "application/json" });
  render(
    <BodyViewer
      api={api}
      flow={flow({ uri: "https://api.example.com/v1/charges?limit=3" })}
      leg="response"
    />,
  );
  await screen.findByTestId("body-json");

  fireEvent.click(screen.getByRole("button", { name: "save" }));

  await waitFor(() => expect(click).toHaveBeenCalled());
  expect(downloads).toEqual(["charges-response.json"]);
  anchor.mockRestore();
  click.mockRestore();
});

test("the five capture absences each state their own cause, and never fetch", () => {
  const cases: [BodyView, string][] = [
    [{ state: "empty" }, "empty body"],
    [{ state: "streaming", content_type: "text/event-stream" }, "streaming (text/event-stream)"],
    [{ state: "not_captured", reason: "too_large", actual: 12_994_392 }, "too large (12 MiB)"],
    [{ state: "not_captured", reason: "read_error", actual: null }, "read error"],
  ];
  for (const [body, expected] of cases) {
    const api = apiWith({});
    const { unmount } = render(
      <BodyViewer api={api} flow={flow({ response_body: body })} leg="response" />,
    );
    expect(screen.getByText(new RegExp(expected.replace(/[()]/g, "\\$&")))).toBeInTheDocument();
    expect(api.getBody).not.toHaveBeenCalled();
    unmount();
  }
});

test("a response that has not arrived says so — 'none' would be a different fact", () => {
  const api = apiWith({});
  render(<BodyViewer api={api} flow={flow({ response_body: null })} leg="response" />);

  expect(screen.getByText("no response yet")).toBeInTheDocument();
  expect(api.getBody).not.toHaveBeenCalled();
});

test("the in-flight state is a notice, not an unstyled paragraph", () => {
  const api = apiWith({ bytes: text("later"), contentType: "text/plain" });
  const { container } = render(<BodyViewer api={api} flow={flow()} leg="response" />);

  const loading = screen.getByText("loading…");
  expect(loading).toBeInTheDocument();
  expect(container.querySelector("[data-tone='mute']")).toContainElement(loading);
});

test("the fetch failure is the only absence in warn tone — it is the only fault", async () => {
  const api = apiWith({});
  (api.getBody as ReturnType<typeof vi.fn>).mockRejectedValue(new Error("boom"));
  const { container } = render(<BodyViewer api={api} flow={flow()} leg="response" />);

  const failure = await screen.findByText("failed to load body");
  expect(container.querySelector("[data-tone='warn']")).toContainElement(failure);
  expect(screen.queryByRole("button", { name: /retry/i })).toBeNull();
});
