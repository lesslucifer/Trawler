import { render, screen, waitFor, fireEvent, act } from "@testing-library/react";
import { afterEach, beforeAll, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import type { Filter, FlowSummary, StreamHandlers, TrawlerApi } from "./api";
import type { OnbState } from "./onboardingMachine";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => {}) }));
// App wires a quit-time close guard (W5); stub the window so onCloseRequested is a
// no-op here. The guard's own behavior is covered in closeConfirm.test.tsx.
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ onCloseRequested: async () => () => {}, destroy: async () => {} }),
}));

// Capture is proven only by a live decrypted flow (W4); until then App shows
// onboarding. These list/filter tests force the proven phase to reach the list.
const PROVEN: OnbState = { phase: "proven", step: "verify", everProven: true };
const ONBOARDING: OnbState = { phase: "onboarding", step: "trust-ca", everProven: false };
const AT_SET_PROXY: OnbState = { phase: "onboarding", step: "set-proxy", everProven: false };

const PEM = "-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----\n";

function onMacHost() {
  Object.defineProperty(window.navigator, "userAgent", {
    configurable: true,
    value: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15",
  });
}

// jsdom reports 0 layout, so react-virtual would render no rows. Give the scroll
// container a real height/width so the virtualizer materializes rows.
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, value: 600 });
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, value: 900 });
  HTMLElement.prototype.getBoundingClientRect = () =>
    ({ height: 600, width: 900, top: 0, left: 0, bottom: 600, right: 900, x: 0, y: 0, toJSON() {} }) as DOMRect;
});

function flow(over: Partial<FlowSummary> = {}): FlowSummary {
  return {
    id: over.id ?? "f1",
    started_at: "2026-07-12T10:00:00Z",
    duration_ms: 5,
    method: "GET",
    uri: "http://example.com/",
    status: 200,
    request_body_state: "empty",
    response_body_state: "empty",
    ...over,
  };
}

// Mock API: `store` is the current server-side result; a filter with host !== ""
// narrows it (mirroring GET /flows). `fire()` triggers a captured SSE listener;
// `setLive(b)` drives the connection-state callback the real EventSource wires.
function mockApi(store: () => FlowSummary[]) {
  let onEvent = () => {};
  let onLive: (live: boolean) => void = () => {};
  const api: TrawlerApi = {
    listFlows: vi.fn(async (filter: Filter) => {
      const rows = store();
      if (filter.host?.trim()) {
        return rows.filter((f) => f.uri.includes(filter.host!.trim()));
      }
      return rows;
    }),
    caCertPem: vi.fn(async () => PEM),
    stream: vi.fn((handlers: StreamHandlers) => {
      onEvent = handlers.onEvent;
      onLive = handlers.onLive ?? (() => {});
      return () => {};
    }),
  };
  return {
    api,
    fire: () => act(() => onEvent()),
    setLive: (live: boolean) => act(() => onLive(live)),
  };
}

afterEach(() => {
  vi.restoreAllMocks();
  delete window.__TRAWLER_PROXY__;
});

test("on mount shows the onboarding surface, not the empty flow list", async () => {
  const { api } = mockApi(() => []);
  render(<App api={api} />);
  expect(await screen.findByText(/Trust the CA/)).toBeInTheDocument();
  expect(screen.queryByTestId("empty")).toBeNull();
});

test("launch with a pre-existing decrypted HTTPS flow goes straight to the list", async () => {
  const { api } = mockApi(() => [flow({ uri: "https://example.com/" })]);
  render(<App api={api} />);
  const row = await screen.findByTestId("flow-row");
  expect(row).toHaveTextContent("example.com");
  expect(screen.queryByText(/Trust the CA/)).toBeNull();
});

test("launch with only a plain-HTTP flow shows onboarding (http:// is not proof)", async () => {
  const { api } = mockApi(() => [flow({ uri: "http://example.com/" })]);
  render(<App api={api} />);
  expect(await screen.findByText(/Trust the CA/)).toBeInTheDocument();
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

test("a decrypted HTTPS flow arriving live flips Verify green and reveals the list", async () => {
  let rows: FlowSummary[] = [];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} />);
  await screen.findByText(/Trust the CA/); // fresh launch → onboarding

  rows = [flow({ uri: "https://example.com/" })];
  fire();
  const row = await screen.findByTestId("flow-row");
  expect(row).toHaveTextContent("example.com");
  expect(screen.queryByText(/Trust the CA/)).toBeNull();
});

test("a plain-HTTP flow arriving live does not flip out of onboarding", async () => {
  let rows: FlowSummary[] = [];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} />);
  await screen.findByText(/Trust the CA/);

  rows = [flow({ uri: "http://example.com/" })];
  fire();
  await waitFor(() => expect(vi.mocked(api.listFlows).mock.calls.length).toBeGreaterThan(1));
  expect(screen.getByText(/Trust the CA/)).toBeInTheDocument();
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

test("a forced-proven phase reveals the flow list", async () => {
  const { api } = mockApi(() => [flow()]);
  render(<App api={api} initialState={PROVEN} />);
  const row = await screen.findByTestId("flow-row");
  expect(row).toHaveTextContent("example.com");
  expect(row.querySelector(".text-ok")).not.toBeNull();
  expect(screen.queryByText(/Trust the CA/)).toBeNull();
});

test("tapping the status dot re-opens setup and stays there despite the captured flow", async () => {
  // A decrypted flow is still present, but re-invoke is a deliberate user action;
  // the live check must not instantly bounce an ever-proven user back to the list.
  const { api } = mockApi(() => [flow({ uri: "https://example.com/" })]);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");
  fireEvent.click(screen.getByTestId("status-dot"));
  expect(await screen.findByText(/Trust the CA/)).toBeInTheDocument();
  expect(screen.queryByTestId("flow-row")).toBeNull();
  expect(screen.getByTestId("onb-exit")).toBeInTheDocument(); // Back to flows, since ever proven
  fireEvent.click(screen.getByTestId("onb-exit"));
  expect(await screen.findByTestId("flow-row")).toBeInTheDocument();
});

test("Install fetches the PEM and invokes install_ca; the step reports done, not proven", async () => {
  onMacHost();
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={ONBOARDING} />);

  fireEvent.click(await screen.findByTestId("install-ca"));
  await screen.findByText(/Verify step will prove/);

  expect(api.caCertPem).toHaveBeenCalledOnce();
  expect(invoke).toHaveBeenCalledWith("install_ca", { pem: PEM });
  // command exit 0 is a weak signal — the machine stays in onboarding until W4 captures
  expect(screen.getByTestId("step-trust-ca")).toHaveAttribute("data-status", "active");
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

test("a rejected install reports failed and keeps the guided steps visible", async () => {
  onMacHost();
  vi.mocked(invoke).mockRejectedValueOnce(new Error("user declined the elevation prompt"));
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={ONBOARDING} />);

  fireEvent.click(await screen.findByTestId("install-ca"));
  await screen.findByText(/follow the steps below/);

  expect(screen.getByTestId("guided-steps")).toHaveTextContent(/security add-trusted-cert/);
  expect(screen.getByTestId("install-ca-error")).toHaveTextContent("user declined the elevation prompt");
  expect(screen.getByTestId("step-trust-ca")).toHaveAttribute("data-status", "active");

  fireEvent.click(screen.getByTestId("onb-next"));
  expect(screen.getByTestId("step-trust-ca")).toHaveTextContent("✗ failed");
  expect(screen.getByTestId("step-trust-ca")).not.toHaveTextContent("✓ done");
});

test("Set system proxy invokes set_proxy with the configured address; the step stays active, not proven", async () => {
  onMacHost();
  window.__TRAWLER_PROXY__ = "127.0.0.1:7999";
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={AT_SET_PROXY} />);

  fireEvent.click(await screen.findByTestId("set-proxy"));
  await screen.findByText(/Verify step will prove/);

  expect(invoke).toHaveBeenCalledWith("set_proxy", { host: "127.0.0.1", port: 7999 });
  expect(screen.getByTestId("step-set-proxy")).toHaveAttribute("data-status", "active");
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

test("a rejected set_proxy reports failed and keeps the snippet visible", async () => {
  onMacHost();
  window.__TRAWLER_PROXY__ = "127.0.0.1:7999";
  vi.mocked(invoke).mockRejectedValueOnce(new Error("no network services to configure"));
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={AT_SET_PROXY} />);

  fireEvent.click(await screen.findByTestId("set-proxy"));
  await screen.findByText(/follow the steps below/);
  expect(screen.getByTestId("proxy-steps")).toHaveTextContent(/networksetup/);
  expect(screen.getByTestId("step-set-proxy")).toHaveAttribute("data-status", "active");
});

test("Stop routing invokes unset_proxy after the proxy was set (W5)", async () => {
  onMacHost();
  window.__TRAWLER_PROXY__ = "127.0.0.1:7999";
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={AT_SET_PROXY} />);

  fireEvent.click(await screen.findByTestId("set-proxy"));
  fireEvent.click(await screen.findByTestId("unset-proxy"));

  await screen.findByText(/Set it again to resume/);
  expect(invoke).toHaveBeenCalledWith("unset_proxy");
});

test("a non-matching host filter empties the list; SSE appends a new row", async () => {
  let rows = [flow({ id: "f1", uri: "http://example.com/" })];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");

  fireEvent.change(screen.getByLabelText("host"), { target: { value: "other.test" } });
  await waitFor(() => expect(screen.queryByTestId("flow-row")).toBeNull());
  expect(screen.getByTestId("empty")).toBeInTheDocument();

  fireEvent.change(screen.getByLabelText("host"), { target: { value: "" } });
  rows = [...rows, flow({ id: "f2", uri: "http://example.com/second", status: 404 })];
  fire();
  await waitFor(() => expect(screen.getAllByTestId("flow-row")).toHaveLength(2));
});
