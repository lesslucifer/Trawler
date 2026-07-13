import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { afterEach, vi } from "vitest";
import { BodyViewer } from "./BodyViewer";
import { hexDump, TREE_MAX } from "./body";
import type { BodyFetch, BodyView, FlowView, TrawlerApi } from "./api";

function flow(over: Partial<FlowView> = {}): FlowView {
  return {
    id: "0192a1b2-c3d4-7e8f-9012-3456789abcde",
    started_at: "2026-07-13T10:00:00Z",
    duration_ms: 5,
    method: "GET",
    uri: "http://example.com/",
    status: 200,
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
  return {
    listFlows: vi.fn(),
    getFlow: vi.fn(),
    stream: vi.fn(),
    getBody: vi.fn(async () => ({
      bytes: new Uint8Array(),
      contentType: null,
      ...body,
    })),
  };
}

const text = (s: string) => new TextEncoder().encode(s);

afterEach(() => vi.unstubAllGlobals());

test("a JSON body renders as a collapsible tree", async () => {
  const api = apiWith({ bytes: text('{"user":{"name":"ada","tags":[1,2]}}'), contentType: "application/json" });
  render(<BodyViewer api={api} flow={flow()} />);
  const tree = await screen.findByTestId("body-json");
  expect(api.getBody).toHaveBeenCalledWith(flow().id, "response", true);
  expect(tree).toHaveTextContent("user");
  expect(tree).toHaveTextContent("name");
  expect(tree).toHaveTextContent('"ada"');
  expect(tree).toHaveTextContent("tags [2]");
  expect(tree.querySelector("details")).not.toBeNull();
});

test("unparseable JSON falls back to plain text", async () => {
  const api = apiWith({ bytes: text("{oops"), contentType: "application/json" });
  render(<BodyViewer api={api} flow={flow()} />);
  expect(await screen.findByTestId("body-text")).toHaveTextContent("{oops");
});

test("an image body renders as an <img> over an object URL", async () => {
  vi.stubGlobal("URL", {
    ...URL,
    createObjectURL: vi.fn(() => "blob:fake"),
    revokeObjectURL: vi.fn(),
  });
  const api = apiWith({ bytes: new Uint8Array([137, 80]), contentType: "image/png" });
  const { unmount } = render(<BodyViewer api={api} flow={flow()} />);
  const img = await screen.findByTestId("body-image");
  expect(img).toHaveAttribute("src", "blob:fake");
  unmount();
});

test("a form body renders as a key/value table", async () => {
  const api = apiWith({
    bytes: text("user=ada&mode=fast%20one"),
    contentType: "application/x-www-form-urlencoded",
  });
  render(<BodyViewer api={api} flow={flow()} />);
  const form = await screen.findByTestId("body-form");
  expect(form).toHaveTextContent("user");
  expect(form).toHaveTextContent("ada");
  expect(form).toHaveTextContent("fast one");
});

test("a binary body renders as a hex dump", async () => {
  const api = apiWith({ bytes: new Uint8Array([0, 1, 2]), contentType: "application/octet-stream" });
  render(<BodyViewer api={api} flow={flow()} />);
  expect(await screen.findByTestId("body-text")).toHaveTextContent("00000000 00 01 02");
});

test("a text body renders as text", async () => {
  const api = apiWith({ bytes: text("plain words"), contentType: "text/plain" });
  render(<BodyViewer api={api} flow={flow()} />);
  expect(await screen.findByTestId("body-text")).toHaveTextContent("plain words");
});

test("the raw toggle refetches undecoded and renders stored bytes as text", async () => {
  const api = apiWith({ bytes: text('{"a":1}'), contentType: "application/json" });
  render(<BodyViewer api={api} flow={flow()} />);
  await screen.findByTestId("body-json");

  fireEvent.click(screen.getByRole("button", { name: "raw" }));
  await screen.findByTestId("body-text");
  expect(api.getBody).toHaveBeenLastCalledWith(flow().id, "response", false);
});

test("the leg tabs switch to the request body", async () => {
  const api = apiWith({ bytes: text("req"), contentType: "text/plain" });
  render(
    <BodyViewer
      api={api}
      flow={flow({ request_body: { state: "captured", size: 3, data_base64: "" } })}
    />,
  );
  await screen.findByTestId("body-text");
  fireEvent.click(screen.getByRole("button", { name: "request" }));
  await waitFor(() => expect(api.getBody).toHaveBeenLastCalledWith(flow().id, "request", true));
});

test("body states show notices, never a blank panel", async () => {
  const cases: [BodyView, string][] = [
    [{ state: "empty" }, "empty body"],
    [{ state: "streaming", content_type: "text/event-stream" }, "streaming"],
    [{ state: "not_captured", reason: "too_large", actual: 9 }, "too large (9 bytes)"],
    [{ state: "not_captured", reason: "read_error", actual: null }, "read error"],
  ];
  for (const [body, expected] of cases) {
    const api = apiWith({});
    const { unmount } = render(<BodyViewer api={api} flow={flow({ response_body: body })} />);
    expect(screen.getByTestId("body-notice")).toHaveTextContent(expected);
    expect(api.getBody).not.toHaveBeenCalled();
    unmount();
  }
});

test("a pending response shows 'no response yet' on the response tab", () => {
  const api = apiWith({});
  render(<BodyViewer api={api} flow={flow({ response_body: null })} />);
  fireEvent.click(screen.getByRole("button", { name: "response" }));
  expect(screen.getByTestId("body-notice")).toHaveTextContent("no response yet");
  expect(api.getBody).not.toHaveBeenCalled();
});

test("a failed body fetch shows an error notice", async () => {
  const api = apiWith({});
  (api.getBody as ReturnType<typeof vi.fn>).mockRejectedValue(new Error("boom"));
  render(<BodyViewer api={api} flow={flow()} />);
  expect(await screen.findByTestId("body-notice")).toHaveTextContent("failed to load body");
});

test("copy body on a hex-rendered body copies the hex dump, not mangled bytes", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  const bytes = new Uint8Array([0, 1, 2]);
  const api = apiWith({ bytes, contentType: "application/octet-stream" });
  render(<BodyViewer api={api} flow={flow()} />);
  await screen.findByTestId("body-text");

  fireEvent.click(screen.getByRole("button", { name: "copy body" }));
  expect(writeText).toHaveBeenCalledWith(hexDump(bytes));
});

test("an image body offers save but not copy (no text representation)", async () => {
  vi.stubGlobal("URL", {
    ...URL,
    createObjectURL: vi.fn(() => "blob:fake"),
    revokeObjectURL: vi.fn(),
  });
  const api = apiWith({ bytes: new Uint8Array([137, 80]), contentType: "image/png" });
  const { unmount } = render(<BodyViewer api={api} flow={flow()} />);
  await screen.findByTestId("body-image");
  expect(screen.queryByRole("button", { name: "copy body" })).toBeNull();
  expect(screen.getByRole("button", { name: "save" })).toBeInTheDocument();
  unmount();
});

test("a JSON body over the tree cap renders as text, never an unbounded tree", async () => {
  const big = `{"pad":"${"x".repeat(TREE_MAX)}"}`;
  const api = apiWith({ bytes: text(big), contentType: "application/json" });
  render(<BodyViewer api={api} flow={flow()} />);
  expect(await screen.findByTestId("body-text")).toBeInTheDocument();
  expect(screen.queryByTestId("body-json")).toBeNull();
});

test("copy body writes the shown body text to the clipboard", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  const api = apiWith({ bytes: text("payload"), contentType: "text/plain" });
  render(<BodyViewer api={api} flow={flow()} />);
  await screen.findByTestId("body-text");

  fireEvent.click(screen.getByRole("button", { name: "copy body" }));
  expect(writeText).toHaveBeenCalledWith("payload");
  await waitFor(() => expect(screen.getByText("copied")).toBeInTheDocument());
});

test("save downloads the shown body under the derived filename", async () => {
  const createObjectURL = vi.fn(() => "blob:save");
  vi.stubGlobal("URL", { ...URL, createObjectURL, revokeObjectURL: vi.fn() });
  const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  const api = apiWith({ bytes: text("payload"), contentType: "text/plain" });
  render(<BodyViewer api={api} flow={flow()} />);
  await screen.findByTestId("body-text");

  fireEvent.click(screen.getByRole("button", { name: "save" }));
  await waitFor(() => expect(click).toHaveBeenCalled());
  expect(createObjectURL).toHaveBeenCalled();
  click.mockRestore();
});
