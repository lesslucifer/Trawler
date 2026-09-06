import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, renderHook, waitFor } from "@testing-library/react";
import { BodyViewer } from "./BodyViewer";
import { FlowList } from "./FlowList";
import { TEXT_MAX } from "./body";
import { TextRenderer } from "./components/TextRenderer";
import { useFlows } from "./useFlows";
import type { BodyFetch, FlowSummary, FlowView, StreamHandlers, TrawlerApi } from "./api";

// Every cost this WU pays is invisible to a rendering assertion — the output was
// always right, it was just re-derived. So the contract is asserted as a count
// of the work done, paired with the rendered output that must not move: a call
// count is the same number on every machine, where a duration is not.
vi.mock("moment", async () => {
  const actual = await vi.importActual<{ default: unknown }>("moment");
  const parse = vi.fn((...args: unknown[]) =>
    (actual.default as (...a: unknown[]) => unknown)(...args),
  );
  Object.assign(parse, actual.default);
  return { ...actual, default: parse, parseSpy: parse };
});

const { parseSpy } = (await import("moment")) as unknown as { parseSpy: ReturnType<typeof vi.fn> };

const flow = (i: number, host: string): FlowSummary => ({
  id: `flow-${i}`,
  started_at: new Date(Date.UTC(2026, 8, 6, 9, 0, 0) + i * 137).toISOString(),
  duration_ms: 12,
  method: "GET",
  uri: `https://${host}/v2/resource/${i}?page=${i % 7}`,
  status: 200,
  request_body_state: "empty",
  response_body_state: "captured",
});

const HOSTS = ["api.example.com", "cdn.example.com:8443", "auth.example.org"];

const FLOWS = Array.from({ length: 40 }, (_, i) => flow(i, HOSTS[i % HOSTS.length]));

// jsdom reports 0 layout, so react-virtual would window down to no rows at all
// and the re-render this asserts over would have nothing to re-render.
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, value: 600 });
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, value: 900 });
  HTMLElement.prototype.getBoundingClientRect = () =>
    ({
      height: 600,
      width: 900,
      top: 0,
      left: 0,
      bottom: 600,
      right: 900,
      x: 0,
      y: 0,
      toJSON() {},
    }) as DOMRect;
});

const times = (container: HTMLElement) =>
  [...container.querySelectorAll('[data-wire="started_at"]')].map((cell) => cell.textContent);

describe("the flow list's time column", () => {
  it("parses each timestamp once however many times the list re-renders", () => {
    const view = render(<FlowList flows={FLOWS} />);
    const first = times(view.container);
    expect(first.length).toBeGreaterThan(0);

    parseSpy.mockClear();
    for (let refresh = 0; refresh < 10; refresh += 1) {
      view.rerender(<FlowList flows={[...FLOWS]} selectedId={null} />);
    }

    expect(parseSpy).not.toHaveBeenCalled();
    expect(times(view.container)).toEqual(first);
  });
});

describe("the live feed's host accumulation", () => {
  const fakeApi = (list: FlowSummary[]) => {
    let handlers: StreamHandlers | undefined;
    const api: TrawlerApi = {
      // A fresh array per call, so a caller can tell one refresh's result from
      // the last one's by identity — the list's contents never change.
      listFlows: async () => [...list],
      connectionCount: async () => 0,
      caCertPem: async () => "",
      getFlow: async () => {
        throw new Error("unused");
      },
      getBody: async () => {
        throw new Error("unused");
      },
      stream: (h) => {
        handlers = h;
        return () => {};
      },
    };
    return { api, fireEvent: () => handlers?.onEvent?.() };
  };

  it("parses no URI on a refresh whose hosts it has already seen", async () => {
    const f = fakeApi(FLOWS);
    const { result } = renderHook(() => useFlows(f.api));

    await waitFor(() => expect(result.current.hosts).toEqual([...HOSTS].sort()));
    const seen = result.current.hosts;
    const settled = result.current.flows;

    const RealURL = globalThis.URL;
    let parses = 0;
    globalThis.URL = class extends RealURL {
      constructor(url: string | URL, base?: string | URL) {
        parses += 1;
        super(url, base);
      }
    } as typeof URL;

    try {
      act(() => f.fireEvent());
      await waitFor(() => expect(result.current.flows).not.toBe(settled));
    } finally {
      globalThis.URL = RealURL;
    }

    expect(parses).toBe(0);
    expect(result.current.hosts).toBe(seen);
  });
});

// Built before the spy exists, so counting the fixture's own encode does not
// masquerade as the component doing it.
const OVER_CAP = "é".repeat(TEXT_MAX + 5000);
const OVER_CAP_BYTES = new TextEncoder().encode(OVER_CAP);

describe("the body pane's size report", () => {
  const real = TextEncoder.prototype.encode;
  let encoded: number[] = [];

  beforeEach(() => {
    encoded = [];
    TextEncoder.prototype.encode = function (this: TextEncoder, input?: string) {
      encoded.push(input?.length ?? 0);
      return real.call(this, input);
    };
  });

  afterEach(() => {
    TextEncoder.prototype.encode = real;
  });

  it("never hands the untruncated body to the encoder when the size is known", () => {
    const { container } = render(
      <TextRenderer text={OVER_CAP} size={2 * (TEXT_MAX + 5000)} />,
    );

    expect(Math.max(...encoded)).toBeLessThanOrEqual(TEXT_MAX);
    expect(container.textContent).toContain("⚠ showing 256 KiB of 266 KiB — save to see all");
  });

  it("BodyViewer supplies the size the fetch already counted", async () => {
    const bytes = OVER_CAP_BYTES;
    const view: FlowView = {
      id: "f1",
      started_at: "2026-09-06T09:00:00Z",
      duration_ms: 12,
      method: "GET",
      uri: "https://api.example.com/big.txt",
      status: 200,
      request_http_version: "HTTP/1.1",
      response_http_version: "HTTP/1.1",
      request_headers: [],
      response_headers: [],
      request_body: { state: "captured", size: bytes.length, data_base64: "" },
      response_body: null,
    };
    const api = {
      getBody: async (): Promise<BodyFetch> => ({ bytes, contentType: "text/plain" }),
    } as unknown as TrawlerApi;

    const { container, findByTestId } = render(
      <BodyViewer api={api} flow={view} leg="request" />,
    );
    await findByTestId("body-text");

    expect(container.textContent).toContain("⚠ showing 256 KiB of 266 KiB — save to see all");
    expect(encoded.length).toBeGreaterThan(0);
    expect(Math.max(...encoded)).toBeLessThanOrEqual(TEXT_MAX);
  });
});
