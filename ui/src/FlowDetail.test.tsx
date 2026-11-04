import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { useState, type ComponentProps } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { FlowDetail } from "./FlowDetail";
import { toCurl } from "./flow";
import { CONFIRM_MS } from "./components/index";
import type { FlowView, Leg, TrawlerApi } from "./api";
import { apiStub } from "./apiStub";
import { DEFAULT_LAYOUT } from "./layoutBounds";
import type { LayoutControls } from "./useLayout";

const still: LayoutControls = {
  layout: DEFAULT_LAYOUT,
  setPane: () => {},
  setHeadersShare: () => {},
  resetPane: () => {},
  resetHeadersShare: () => {},
  setColumn: () => {},
  resetColumn: () => {},
  commit: () => {},
};

type Chosen = "leg" | "onLeg" | "pretty" | "onPretty";

function Detail(props: Omit<ComponentProps<typeof FlowDetail>, Chosen> & { initialLeg?: Leg }) {
  const { initialLeg = "request", ...rest } = props;
  const [leg, setLeg] = useState<Leg>(initialLeg);
  const [pretty, setPretty] = useState(true);
  return <FlowDetail {...rest} leg={leg} onLeg={setLeg} pretty={pretty} onPretty={setPretty} />;
}

const stubApi = (): TrawlerApi =>
  apiStub({
    getBody: vi.fn(async () => ({
      bytes: new TextEncoder().encode("body!"),
      contentType: "text/plain",
    })),
  });

function view(over: Partial<FlowView> = {}): FlowView {
  return {
    id: "f1",
    started_at: "2026-07-12T10:00:00Z",
    duration_ms: 42,
    method: "GET",
    uri: "http://example.com/a?x=1",
    status: 200,
    source: "captured",
    seed: null,
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
  render(<Detail split={still} api={stubApi()} flow={null} onClose={() => {}} />);
  const pending = screen.getByTestId("detail-loading");
  expect(pending).toHaveTextContent("loading this flow…");
  expect(pending.querySelector("[data-tone]")).toHaveAttribute("data-tone", "mute");
  expect(screen.queryByRole("radiogroup")).toBeNull();
  expect(screen.queryByTestId("detail")).toBeNull();
});

test("the identity row: verb, status with its reason phrase, duration, and the leg selector on request", () => {
  render(<Detail split={still} api={stubApi()} flow={view()} onClose={() => {}} />);
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
  render(<Detail split={still} api={stubApi()} flow={view({ duration_ms: 1400 })} onClose={() => {}} />);
  expect(screen.getByText("1.4 s")).toBeInTheDocument();
});

test("the flow line: the URI is a copy target and the versions line carries both protocols", async () => {
  const writeText = stubClipboard();
  render(<Detail split={still} api={stubApi()} flow={view()} onClose={() => {}} />);
  const uri = screen.getByRole("button", { name: "copy uri" });
  expect(uri).toHaveTextContent("http://example.com/a?x=1");
  expect(screen.getByTestId("flow-line")).toHaveTextContent("HTTP/1.1 → HTTP/2.0");

  fireEvent.click(uri);
  expect(writeText).toHaveBeenCalledWith("http://example.com/a?x=1");
  await waitFor(() => expect(screen.getByText("copied")).toBeInTheDocument());
});

test("an in-flight flow has no response version and the line carries no arrow", () => {
  render(
    <Detail split={still}
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
    <Detail split={still}
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
  render(<Detail split={still} api={stubApi()} flow={view({ response_headers: [] })} onClose={() => {}} />);
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
    render(<Detail split={still} api={stubApi()} flow={flow} onClose={() => {}} />);
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
  render(<Detail split={still} api={stubApi()} flow={view()} onClose={onClose} />);
  fireEvent.click(screen.getByRole("button", { name: "close" }));
  expect(onClose).toHaveBeenCalledTimes(1);
});

test("Esc closes the panel from anywhere, unless another surface already claimed it", () => {
  const onClose = vi.fn();
  render(<Detail split={still} api={stubApi()} flow={view()} onClose={onClose} />);

  fireEvent.keyDown(document.body, { key: "Escape" });
  expect(onClose).toHaveBeenCalledTimes(1);

  const claimed = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  claimed.preventDefault();
  document.body.dispatchEvent(claimed);
  expect(onClose).toHaveBeenCalledTimes(1);
});

test("Esc still dismisses a panel that is only loading", () => {
  const onClose = vi.fn();
  render(<Detail split={still} api={stubApi()} flow={null} onClose={onClose} />);
  fireEvent.keyDown(document.body, { key: "Escape" });
  expect(onClose).toHaveBeenCalledTimes(1);
});

test("closing returns focus to whatever had it when the panel opened", () => {
  const origin = document.createElement("button");
  document.body.appendChild(origin);
  origin.focus();
  try {
    render(<Detail split={still} api={stubApi()} flow={view()} onClose={() => {}} />);
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
    <Detail split={still}
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
    <Detail split={still}
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
  expect(screen.getByTestId("headers-zone")).toHaveStyle({ maxHeight: "35%" });
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
      <Detail split={still}
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

test("replay stands in the flow line beside copy as curl, and opens a draft of this flow", () => {
  const onReplay = vi.fn();
  render(<Detail split={still} api={stubApi()} flow={view()} onClose={() => {}} onReplay={onReplay} />);
  const line = screen.getByTestId("flow-line");

  const buttons = within(line).getAllByRole("button");
  const replay = within(line).getByRole("button", { name: "replay" });
  expect(buttons.indexOf(replay)).toBe(buttons.indexOf(within(line).getByRole("button", { name: "copy as curl" })) - 1);
  fireEvent.click(within(line).getByRole("button", { name: "replay" }));
  expect(onReplay).toHaveBeenCalledOnce();
});

test("with nothing to replay into, the inspector offers no replay", () => {
  render(<Detail split={still} api={stubApi()} flow={view()} onClose={() => {}} />);
  expect(screen.queryByRole("button", { name: "replay" })).toBeNull();
});

test("the headers ceiling is the user's share, sized by a splitter under the headers", () => {
  const setHeadersShare = vi.fn();
  const commit = vi.fn();
  const resetHeadersShare = vi.fn();
  render(
    <Detail
      split={{ ...still, layout: { ...DEFAULT_LAYOUT, headersShare: 0.5 }, setHeadersShare, commit, resetHeadersShare }}
      api={stubApi()}
      flow={view()}
      onClose={() => {}}
    />,
  );
  expect(screen.getByTestId("headers-zone")).toHaveStyle({ maxHeight: "50%" });

  const splitter = screen.getByRole("separator", { name: "headers height" });
  expect(splitter).toHaveAttribute("aria-orientation", "horizontal");
  expect(splitter).toHaveAttribute("aria-valuemin", "0.15");
  expect(splitter).toHaveAttribute("aria-valuemax", "0.6");
  expect(splitter).toHaveAttribute("aria-valuetext", "50 % of the pane");

  fireEvent.keyDown(splitter, { key: "End" });
  expect(setHeadersShare).toHaveBeenLastCalledWith(0.6);
  expect(commit).toHaveBeenCalledTimes(1);
  fireEvent.doubleClick(splitter);
  expect(resetHeadersShare).toHaveBeenCalledTimes(1);
});

test("the pane's height turns a dragged pixel into a share", () => {
  const setHeadersShare = vi.fn();
  const tall = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ height: 400 } as DOMRect);
  try {
    render(<Detail split={{ ...still, setHeadersShare }} api={stubApi()} flow={view()} onClose={() => {}} />);
    const splitter = screen.getByRole("separator", { name: "headers height" });
    fireEvent.pointerDown(splitter, { button: 0, clientY: 300, pointerId: 1 });
    fireEvent.pointerMove(splitter, { clientY: 340, pointerId: 1 });
    expect(setHeadersShare.mock.lastCall?.[0]).toBeCloseTo(0.45);
  } finally {
    tall.mockRestore();
  }
});

test("splitter B starts from where a short leg puts it, not from the ceiling above it", () => {
  const setHeadersShare = vi.fn();
  const heights = vi
    .spyOn(HTMLElement.prototype, "getBoundingClientRect")
    .mockImplementation(function (this: HTMLElement) {
      return { height: this.dataset.testid === "headers-zone" ? 80 : 400 } as DOMRect;
    });
  try {
    render(
      <Detail
        split={{ ...still, layout: { ...DEFAULT_LAYOUT, headersShare: 0.5 }, setHeadersShare }}
        api={stubApi()}
        flow={view()}
        onClose={() => {}}
      />,
    );
    const splitter = screen.getByRole("separator", { name: "headers height" });
    expect(splitter).toHaveAttribute("aria-valuenow", "0.2");
    expect(splitter).toHaveAttribute("aria-valuetext", "20 % of the pane");
    expect(screen.getByTestId("headers-zone")).toHaveStyle({ maxHeight: "50%" });
    fireEvent.keyDown(splitter, { key: "ArrowUp" });
    expect(setHeadersShare.mock.lastCall?.[0]).toBeCloseTo(0.18);
  } finally {
    heights.mockRestore();
  }
});

test("a leg shorter than zone-min keeps its line where it sits, and the share saved never drops below zone-min", () => {
  const setHeadersShare = vi.fn();
  const heights = vi
    .spyOn(HTMLElement.prototype, "getBoundingClientRect")
    .mockImplementation(function (this: HTMLElement) {
      return { height: this.dataset.testid === "headers-zone" ? 32 : 400 } as DOMRect;
    });
  try {
    render(<Detail split={{ ...still, setHeadersShare }} api={stubApi()} flow={view()} onClose={() => {}} />);
    const splitter = screen.getByRole("separator", { name: "headers height" });
    expect(splitter).toHaveAttribute("aria-valuenow", "0.08");
    expect(splitter).toHaveAttribute("aria-valuemin", "0.08");
    fireEvent.keyDown(splitter, { key: "ArrowDown" });
    expect(setHeadersShare).toHaveBeenLastCalledWith(0.15);
    fireEvent.pointerDown(splitter, { button: 0, clientY: 100, pointerId: 1 });
    fireEvent.pointerMove(splitter, { clientY: 180, pointerId: 1 });
    expect(setHeadersShare.mock.lastCall?.[0]).toBeCloseTo(0.28);
  } finally {
    heights.mockRestore();
  }
});

test("resizing the headers does not decode the body again", async () => {
  const api = apiStub({
    getBody: vi.fn(async () => ({ bytes: new TextEncoder().encode('{"a":1}'), contentType: "application/json" })),
  });
  const flow = view({ request_body: { state: "captured", size: 7, data_base64: "" } });
  const { rerender } = render(<Detail split={still} api={api} flow={flow} onClose={() => {}} />);
  await screen.findByTestId("body-pane");
  const decode = vi.spyOn(TextDecoder.prototype, "decode");
  try {
    for (const headersShare of [0.4, 0.45, 0.5]) {
      rerender(
        <Detail
          split={{ ...still, layout: { ...DEFAULT_LAYOUT, headersShare } }}
          api={api}
          flow={flow}
          onClose={() => {}}
        />,
      );
    }
    expect(decode).not.toHaveBeenCalled();
  } finally {
    decode.mockRestore();
  }
});

// E-STICKY: the leg is the user's, held above the pane, so the pane opens on it
// and hands a pick back instead of keeping one of its own.
test("the pane opens on the leg it is handed and hands a pick back without taking it", () => {
  const onLeg = vi.fn();
  render(
    <FlowDetail
      split={still}
      api={stubApi()}
      flow={view()}
      onClose={() => {}}
      leg="response"
      onLeg={onLeg}
      pretty={false}
      onPretty={() => {}}
    />,
  );
  expect(legRadio("response")).toBeChecked();
  expect(screen.getByRole("radiogroup", { name: "response body mode" })).toBeInTheDocument();
  fireEvent.click(legRadio("request"));
  expect(onLeg).toHaveBeenCalledWith("request");
  expect(legRadio("response")).toBeChecked();
});

test("a flow with no response yet still opens on the response leg the user chose", () => {
  render(
    <Detail
      split={still}
      api={stubApi()}
      flow={view({ status: null, response_http_version: null, response_body: null, response_headers: [] })}
      onClose={() => {}}
      initialLeg="response"
    />,
  );
  expect(legRadio("response")).toBeChecked();
  expect(screen.getByTestId("body-viewer")).toHaveAttribute("data-leg", "response");
  expect(screen.getByText("no response yet")).toBeInTheDocument();
});
