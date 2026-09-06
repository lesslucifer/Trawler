import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { FlowDetail } from "./FlowDetail";
import { toCurl } from "./flow";
import { CONFIRM_MS } from "./components/index";
import type { FlowView, TrawlerApi } from "./api";

const stubApi = (): TrawlerApi => ({
  listFlows: vi.fn(),
  connectionCount: vi.fn(async () => 0),
  caCertPem: vi.fn(async () => ""),
  getFlow: vi.fn(),
  getBody: vi.fn(async () => ({ bytes: new TextEncoder().encode("body!"), contentType: "text/plain" })),
  stream: vi.fn(),
});

function view(over: Partial<FlowView> = {}): FlowView {
  return {
    id: "f1",
    started_at: "2026-07-12T10:00:00Z",
    duration_ms: 42,
    method: "GET",
    uri: "http://example.com/a?x=1",
    status: 200,
    request_http_version: "HTTP/1.1",
    response_http_version: "HTTP/2.0",
    request_headers: [{ name: "Accept", value: "application/json" }],
    response_headers: [{ name: "Content-Type", value: "application/json" }],
    request_body: { state: "empty" },
    response_body: { state: "captured", size: 12, data_base64: "" },
    ...over,
  };
}

function stubClipboard() {
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  return writeText;
}

const legSelector = () => screen.getByRole("radiogroup", { name: "leg" });
const legRadio = (leg: "request" | "response") => within(legSelector()).getByRole("radio", { name: leg });

afterEach(() => vi.unstubAllGlobals());

// jsdom lays nothing out, so a value cell's overflow is stated for it: every
// <dd> is 60px tall with 200px of content, which is what a clamped cookie is.
function withOverflowingValueCells(run: () => void) {
  Object.defineProperty(HTMLElement.prototype, "scrollHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return this.tagName === "DD" ? 200 : 0;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return this.tagName === "DD" ? 60 : 0;
    },
  });
  try {
    run();
  } finally {
    delete (HTMLElement.prototype as { scrollHeight?: number }).scrollHeight;
    delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
  }
}

test("while the flow is still loading, one sentence stands in for the whole panel", () => {
  render(<FlowDetail api={stubApi()} flow={null} onClose={() => {}} />);
  const pending = screen.getByTestId("detail-loading");
  expect(pending).toHaveTextContent("loading this flow…");
  expect(pending.querySelector("[data-tone]")).toHaveAttribute("data-tone", "mute");
  expect(screen.queryByRole("radiogroup")).toBeNull();
  expect(screen.queryByTestId("detail")).toBeNull();
});

test("the identity row: verb, status with its reason phrase, duration, and the leg selector on request", () => {
  render(<FlowDetail api={stubApi()} flow={view()} onClose={() => {}} />);
  const detail = screen.getByTestId("detail");
  expect(within(detail.querySelector("header")!).getByText("GET").className).toMatch(/text-ok/);
  expect(screen.getByLabelText("200 OK")).toBeInTheDocument();
  expect(screen.getByText("42 ms")).toHaveAttribute("aria-label", "42 milliseconds");
  expect(legRadio("request")).toHaveAttribute("aria-checked", "true");
  expect(legRadio("response")).toHaveAttribute("aria-checked", "false");
  expect(screen.getAllByRole("radiogroup", { name: /leg|mode/ })).toHaveLength(2);
  expect(screen.queryByText(/request headers/i)).toBeNull();
  expect(screen.queryByText(/response headers/i)).toBeNull();
});

test("a duration over a second is scaled by the component, not by the screen", () => {
  render(<FlowDetail api={stubApi()} flow={view({ duration_ms: 1400 })} onClose={() => {}} />);
  expect(screen.getByText("1.4 s")).toBeInTheDocument();
});

test("the flow line: the URI is a copy target and the versions line carries both protocols", async () => {
  const writeText = stubClipboard();
  render(<FlowDetail api={stubApi()} flow={view()} onClose={() => {}} />);
  const uri = screen.getByRole("button", { name: "copy uri" });
  expect(uri).toHaveTextContent("http://example.com/a?x=1");
  expect(screen.getByTestId("flow-line")).toHaveTextContent("HTTP/1.1 → HTTP/2.0");

  fireEvent.click(uri);
  expect(writeText).toHaveBeenCalledWith("http://example.com/a?x=1");
  await waitFor(() => expect(screen.getByText("copied")).toBeInTheDocument());
});

test("an in-flight flow has no response version and the line carries no arrow", () => {
  render(
    <FlowDetail
      api={stubApi()}
      flow={view({ response_http_version: null, status: null, duration_ms: null, response_body: null })}
      onClose={() => {}}
    />,
  );
  const line = screen.getByTestId("flow-line");
  expect(line).toHaveTextContent("HTTP/1.1");
  expect(line.textContent).not.toContain("→");
});

test("headers render in wire order, duplicate names survive, and a value copies on click", async () => {
  const writeText = stubClipboard();
  render(
    <FlowDetail
      api={stubApi()}
      flow={view({
        request_headers: [
          { name: "set-cookie", value: "a=1" },
          { name: "host", value: "example.com" },
          { name: "set-cookie", value: "b=2" },
          { name: "set-cookie", value: "c=3" },
        ],
      })}
      onClose={() => {}}
    />,
  );
  const table = screen.getByTestId("detail").querySelector('dl[data-order="wire"]')!;
  expect([...table.querySelectorAll("dt")].map((dt) => dt.textContent)).toEqual([
    "set-cookie",
    "host",
    "set-cookie",
    "set-cookie",
  ]);

  fireEvent.click(screen.getByText("b=2"));
  expect(writeText).toHaveBeenCalledWith("b=2");
  await waitFor(() => expect(screen.getByText("copied")).toBeInTheDocument());
});

test("a leg that sent no headers says so, keeps its zone, and stays selectable", () => {
  render(<FlowDetail api={stubApi()} flow={view({ response_headers: [] })} onClose={() => {}} />);
  fireEvent.click(legRadio("response"));
  const notice = screen.getByText("none — this leg sent no headers");
  expect(notice.closest("[data-tone]")).toHaveAttribute("data-tone", "mute");
  expect(screen.getByRole("heading", { name: "Headers · 0" })).toBeInTheDocument();
  expect(legRadio("response")).not.toBeDisabled();
  expect(legRadio("request")).not.toBeDisabled();
});

test("copy as curl writes the command, confirms, reverts — and the command is never on screen", async () => {
  vi.useFakeTimers();
  try {
    const writeText = stubClipboard();
    const flow = view();
    render(<FlowDetail api={stubApi()} flow={flow} onClose={() => {}} />);
    expect(screen.getByTestId("detail").textContent).not.toContain("curl '");

    fireEvent.click(screen.getByRole("button", { name: "copy as curl" }));
    expect(writeText).toHaveBeenCalledWith(toCurl(flow));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(screen.getByRole("button", { name: /copied/ })).toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(CONFIRM_MS + 1);
    });
    expect(screen.getByRole("button", { name: "copy as curl" })).toBeInTheDocument();
    expect(screen.queryByText("copied")).toBeNull();
  } finally {
    vi.useRealTimers();
  }
});

test("✕ closes the panel", () => {
  const onClose = vi.fn();
  render(<FlowDetail api={stubApi()} flow={view()} onClose={onClose} />);
  fireEvent.click(screen.getByRole("button", { name: "close" }));
  expect(onClose).toHaveBeenCalledTimes(1);
});

test("Esc closes the panel from anywhere, unless another surface already claimed it", () => {
  const onClose = vi.fn();
  render(<FlowDetail api={stubApi()} flow={view()} onClose={onClose} />);

  fireEvent.keyDown(document.body, { key: "Escape" });
  expect(onClose).toHaveBeenCalledTimes(1);

  const claimed = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  claimed.preventDefault();
  document.body.dispatchEvent(claimed);
  expect(onClose).toHaveBeenCalledTimes(1);
});

test("Esc still dismisses a panel that is only loading", () => {
  const onClose = vi.fn();
  render(<FlowDetail api={stubApi()} flow={null} onClose={onClose} />);
  fireEvent.keyDown(document.body, { key: "Escape" });
  expect(onClose).toHaveBeenCalledTimes(1);
});

test("closing returns focus to whatever had it when the panel opened", () => {
  const origin = document.createElement("button");
  document.body.appendChild(origin);
  origin.focus();
  try {
    render(<FlowDetail api={stubApi()} flow={view()} onClose={() => {}} />);
    const close = screen.getByRole("button", { name: "close" });
    close.focus();
    expect(document.activeElement).toBe(close);

    fireEvent.click(close);
    expect(document.activeElement).toBe(origin);
  } finally {
    origin.remove();
  }
});

// The WU's bounded contract, and the one no unit test on either zone can reach:
// a single selector moves the header table AND the body pane to the same leg,
// while the flow-scoped line does not move at all.
test("one selector governs both leg-scoped zones; the flow line follows neither", async () => {
  const api = stubApi();
  render(
    <FlowDetail
      api={api}
      flow={view({
        request_headers: [{ name: "accept", value: "*/*" }],
        response_headers: [{ name: "content-type", value: "text/plain" }],
        request_body: { state: "captured", size: 5, data_base64: "" },
      })}
      onClose={() => {}}
    />,
  );
  await screen.findByTestId("body-text");
  expect(screen.getByText("accept")).toBeInTheDocument();
  expect(screen.queryByText("content-type")).toBeNull();
  expect(screen.getByTestId("body-viewer")).toHaveAttribute("data-leg", "request");
  expect(api.getBody).toHaveBeenCalledWith("f1", "request", true);
  expect(api.getBody).not.toHaveBeenCalledWith("f1", "response", true);
  const flowLineBefore = screen.getByTestId("flow-line").textContent;
  const requestFetches = (api.getBody as ReturnType<typeof vi.fn>).mock.calls.length;

  fireEvent.click(legRadio("response"));

  expect(screen.getByText("content-type")).toBeInTheDocument();
  expect(screen.queryByText("accept")).toBeNull();
  expect(screen.getAllByTestId("body-viewer")).toHaveLength(1);
  expect(screen.getByTestId("body-viewer")).toHaveAttribute("data-leg", "response");
  await waitFor(() => expect(api.getBody).toHaveBeenCalledWith("f1", "response", true));
  expect((api.getBody as ReturnType<typeof vi.fn>).mock.calls.length).toBe(requestFetches + 1);
  expect(screen.getByTestId("flow-line").textContent).toBe(flowLineBefore);
});

// The owner's round: with real traffic the header table buried the body under
// cookies and tokens, and the panel scrolled as one document to reach it. The
// body is the zone a reader came for, so it takes the height the headers leave.
test("the panel never scrolls as a whole: headers are capped and scroll inside, the body fills the rest", async () => {
  render(
    <FlowDetail
      api={stubApi()}
      flow={view({ request_body: { state: "captured", size: 5, data_base64: "" } })}
      onClose={() => {}}
    />,
  );
  const detail = screen.getByTestId("detail");
  expect(detail.querySelector(":scope > div.overflow-auto")).toBeNull();

  const headersBox = screen.getByTestId("headers-scroll");
  expect(headersBox.className).toMatch(/overflow-auto/);
  expect(headersBox.className).toMatch(/relative/);
  expect(headersBox.closest("section")!.className).toMatch(/max-h-zone-cap/);
  expect(screen.getByRole("heading", { name: "Headers · 1" })).toBeInTheDocument();

  const bodySection = screen.getByRole("heading", { name: "Body" }).closest("section")!;
  expect(bodySection.className).toMatch(/flex-1/);
  expect(bodySection.className).toMatch(/min-h-0/);
  const pane = await screen.findByTestId("body-pane");
  expect(pane.className).toMatch(/flex-1/);
  expect(pane.className).toMatch(/relative/);
  expect(pane.className).not.toMatch(/max-h-/);
});

test("a header value taller than three lines is clamped, and one act under its name shows the rest", () => {
  const cookie = "session=" + "x".repeat(900);
  withOverflowingValueCells(() => {
    render(
      <FlowDetail
        api={stubApi()}
        flow={view({ request_headers: [{ name: "set-cookie", value: cookie }] })}
        onClose={() => {}}
      />,
    );
    const cell = screen.getByText(cookie).closest("dd")!;
    expect(cell.className).toMatch(/max-h-value-clamp/);
    expect(screen.getByRole("button", { name: "copy set-cookie" })).toHaveTextContent(cookie);

    fireEvent.click(screen.getByRole("button", { name: "more of set-cookie" }));
    expect(cell.className).not.toMatch(/max-h-value-clamp/);
    expect(screen.getByRole("button", { name: "less of set-cookie" })).toBeInTheDocument();
  });
});
