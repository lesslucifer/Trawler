import { render, screen, waitFor, fireEvent, act } from "@testing-library/react";
import { afterEach, beforeAll, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import type { Filter, FlowSummary, StreamHandlers, TrawlerApi } from "./api";
import type { OnbState } from "./onboardingMachine";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => {}) }));

// Capture is proven only by a live decrypted flow (W4); until then App shows
// onboarding. These list/filter tests force the proven phase to reach the list.
const PROVEN: OnbState = { phase: "proven", step: "verify", everProven: true };
const ONBOARDING: OnbState = { phase: "onboarding", step: "trust-ca", everProven: false };

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

afterEach(() => vi.restoreAllMocks());

test("on mount shows the onboarding surface, not the empty flow list", async () => {
  const { api } = mockApi(() => []);
  render(<App api={api} />);
  expect(await screen.findByText(/Trust the CA/)).toBeInTheDocument();
  expect(screen.queryByTestId("empty")).toBeNull();
});

test("a forced-proven phase reveals the flow list", async () => {
  const { api } = mockApi(() => [flow()]);
  render(<App api={api} initialState={PROVEN} />);
  const row = await screen.findByTestId("flow-row");
  expect(row).toHaveTextContent("example.com");
  expect(row.querySelector(".text-ok")).not.toBeNull();
  expect(screen.queryByText(/Trust the CA/)).toBeNull();
});

test("tapping the status dot re-opens setup from the flow list", async () => {
  const { api } = mockApi(() => [flow()]);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");
  fireEvent.click(screen.getByTestId("status-dot"));
  expect(await screen.findByText(/Trust the CA/)).toBeInTheDocument();
  expect(screen.queryByTestId("flow-row")).toBeNull();
  expect(screen.getByTestId("onb-exit")).toBeInTheDocument(); // Back to flows, since ever proven
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
  expect(screen.getByTestId("step-trust-ca")).toHaveAttribute("data-status", "active");
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
