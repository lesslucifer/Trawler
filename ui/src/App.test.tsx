import { render, screen, waitFor, fireEvent, act } from "@testing-library/react";
import { afterEach, beforeAll, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import type { Filter, FlowSummary, FlowView, StreamHandlers, TrawlerApi } from "./api";
import type { OnbState } from "./onboardingMachine";
import { CA_FILENAME, guidedSteps } from "./setup";

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
// narrows it (mirroring GET /flows). `fire()` triggers a captured SSE listener.
function mockApi(store: () => FlowSummary[], connections = 0) {
  let onEvent = () => {};
  let onConnection = () => {};
  const api: TrawlerApi = {
    listFlows: vi.fn(async (filter: Filter) => {
      // Close enough to FlowFilter::matches (store.rs) for these tests: q really
      // is a case-insensitive substring of the URI, while host is matched there
      // against the parsed host alone. The fixtures put each host in exactly one
      // flow's authority, so the two agree here.
      const like = (uri: string, needle?: string) =>
        !needle?.trim() || uri.toLowerCase().includes(needle.trim().toLowerCase());
      return store().filter((f) => like(f.uri, filter.host) && like(f.uri, filter.q));
    }),
    connectionCount: vi.fn(async () => connections),
    caCertPem: vi.fn(async () => PEM),
    getFlow: vi.fn(async (id: string): Promise<FlowView> => {
      const summary = store().find((f) => f.id === id)!;
      return {
        ...summary,
        request_http_version: "HTTP/1.1",
        response_http_version: "HTTP/1.1",
        request_headers: [{ name: "Accept", value: "application/json" }],
        response_headers: [],
        request_body: { state: "empty" },
        response_body: { state: "empty" },
      };
    }),
    getBody: vi.fn(async () => ({
      bytes: new TextEncoder().encode('{"greeting":"hello"}'),
      contentType: "application/json",
    })),
    stream: vi.fn((handlers: StreamHandlers) => {
      onEvent = handlers.onEvent;
      onConnection = handlers.onConnection ?? (() => {});
      return () => {};
    }),
  };
  return {
    api,
    fire: () => act(() => onEvent()),
    fireConnection: () => act(() => onConnection()),
  };
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  delete window.__TRAWLER_PROXY__;
});

test("on mount shows the onboarding surface, not the empty flow list", async () => {
  const { api } = mockApi(() => []);
  render(<App api={api} />);
  expect(await screen.findByText(/trust the CA/)).toBeInTheDocument();
  expect(screen.queryByTestId("empty")).toBeNull();
});

test("launch with a pre-existing decrypted HTTPS flow goes straight to the list", async () => {
  const { api } = mockApi(() => [flow({ uri: "https://example.com/" })]);
  render(<App api={api} />);
  const row = await screen.findByTestId("flow-row");
  expect(row).toHaveTextContent("example.com");
  expect(screen.queryByText(/trust the CA/)).toBeNull();
});

test("launch with only a plain-HTTP flow shows onboarding (http:// is not proof)", async () => {
  const { api } = mockApi(() => [flow({ uri: "http://example.com/" })]);
  render(<App api={api} />);
  expect(await screen.findByText(/trust the CA/)).toBeInTheDocument();
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

test("a decrypted HTTPS flow arriving live flips Verify green and reveals the list", async () => {
  let rows: FlowSummary[] = [];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} />);
  await screen.findByText(/trust the CA/); // fresh launch → onboarding

  rows = [flow({ uri: "https://example.com/" })];
  fire();
  const row = await screen.findByTestId("flow-row");
  expect(row).toHaveTextContent("example.com");
  expect(screen.queryByText(/trust the CA/)).toBeNull();
});

test("a plain-HTTP flow arriving live does not flip out of onboarding", async () => {
  let rows: FlowSummary[] = [];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} />);
  await screen.findByText(/trust the CA/);

  rows = [flow({ uri: "http://example.com/" })];
  fire();
  await waitFor(() => expect(vi.mocked(api.listFlows).mock.calls.length).toBeGreaterThan(1));
  expect(screen.getByText(/trust the CA/)).toBeInTheDocument();
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

test("a forced-proven phase reveals the flow list", async () => {
  const { api } = mockApi(() => [flow()]);
  render(<App api={api} initialState={PROVEN} />);
  const row = await screen.findByTestId("flow-row");
  expect(row).toHaveTextContent("example.com");
  expect(row.querySelector(".text-ok")).not.toBeNull();
  expect(screen.queryByText(/trust the CA/)).toBeNull();
});

test("tapping the status dot re-opens setup and stays there despite the captured flow", async () => {
  // A decrypted flow is still present, but re-invoke is a deliberate user action;
  // the live check must not instantly bounce an ever-proven user back to the list.
  const { api } = mockApi(() => [flow({ uri: "https://example.com/" })]);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");
  fireEvent.click(screen.getByTestId("status-dot"));
  expect(await screen.findByText(/trust the CA/)).toBeInTheDocument();
  expect(screen.queryByTestId("flow-row")).toBeNull();
  expect(screen.getByRole("button", { name: /back to flows/ })).toBeInTheDocument(); // Back to flows, since ever proven
  fireEvent.click(screen.getByRole("button", { name: /back to flows/ }));
  expect(await screen.findByTestId("flow-row")).toBeInTheDocument();
});

test("Install fetches the PEM and invokes install_ca; the step reports done, not proven", async () => {
  onMacHost();
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={ONBOARDING} />);

  fireEvent.click(await screen.findByRole("button", { name: "install certificate" }));
  await screen.findByText(/we'll confirm it on your first flow/);

  expect(api.caCertPem).toHaveBeenCalledOnce();
  expect(invoke).toHaveBeenCalledWith("install_ca", { pem: PEM });
  // command exit 0 is a weak signal — the machine stays in onboarding until W4 captures
  expect(screen.getByTestId("step-trust-ca")).toHaveAttribute("data-status", "active");
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

// The manual steps tell the user to run commands against a file by name. The
// save has to produce that exact name, or every step after it points at nothing.
test("Save writes the PEM under the filename the manual steps tell the user to run against", async () => {
  onMacHost();
  const { api } = mockApi(() => []);
  const saved: { name?: string; size?: number } = {};
  vi.stubGlobal("URL", { ...URL, createObjectURL: vi.fn(() => "blob:ca"), revokeObjectURL: vi.fn() });
  const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  const anchorName = vi
    .spyOn(HTMLAnchorElement.prototype, "download", "set")
    .mockImplementation(function (this: HTMLAnchorElement, name: string) {
      saved.name = name;
    });
  render(<App api={api} initialState={ONBOARDING} />);

  fireEvent.click(await screen.findByRole("button", { name: "save certificate" }));
  await screen.findByText(new RegExp(`saved as ${CA_FILENAME}`));

  expect(api.caCertPem).toHaveBeenCalledOnce();
  expect(saved.name).toBe(CA_FILENAME);
  for (const step of guidedSteps("macos")) {
    if (/certificate as /.test(step)) expect(step).toContain(saved.name!);
  }
  anchorName.mockRestore();
  click.mockRestore();
});

// Reject one specific command, letting the read-only probes (ca_trusted /
// proxy_status) resolve normally — they fire on entry, so a blanket
// mockRejectedValueOnce would be consumed by a probe, not the action.
function rejectCommand(cmd: string, error: Error) {
  vi.mocked(invoke).mockImplementation(async (name: string) => {
    if (name === cmd) throw error;
    return undefined;
  });
}

test("a rejected install reports failed and keeps the guided steps visible", async () => {
  onMacHost();
  rejectCommand("install_ca", new Error("user declined the elevation prompt"));
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={ONBOARDING} />);

  fireEvent.click(await screen.findByRole("button", { name: "install certificate" }));
  await screen.findByText(/use the manual steps below/);

  expect(screen.getByTestId("guided-steps")).toHaveTextContent(/security add-trusted-cert/);
  expect(screen.getByTestId("install-ca-status")).toHaveTextContent("user declined the elevation prompt");
  expect(screen.getByTestId("step-trust-ca")).toHaveAttribute("data-status", "active");

  const status = screen.getByTestId("step-trust-ca-status");
  expect(status).toHaveTextContent(/install failed/);
  expect(status).not.toHaveTextContent(/trusted on this machine/);
});

test("Set system proxy invokes set_proxy with the configured address; the step stays active, not proven", async () => {
  onMacHost();
  window.__TRAWLER_PROXY__ = "127.0.0.1:7999";
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={AT_SET_PROXY} />);

  fireEvent.click(await screen.findByRole("button", { name: "set system proxy" }));
  await screen.findByText(/we'll confirm it on your first flow/);

  expect(invoke).toHaveBeenCalledWith("set_proxy", { host: "127.0.0.1", port: 7999 });
  expect(screen.getByTestId("step-set-proxy")).toHaveAttribute("data-status", "active");
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

test("a rejected set_proxy reports failed and keeps the snippet visible", async () => {
  onMacHost();
  window.__TRAWLER_PROXY__ = "127.0.0.1:7999";
  rejectCommand("set_proxy", new Error("no network services to configure"));
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={AT_SET_PROXY} />);

  fireEvent.click(await screen.findByRole("button", { name: "set system proxy" }));
  await screen.findByText(/use the manual steps below/);
  expect(screen.getByTestId("proxy-steps")).toHaveTextContent(/networksetup/);
  expect(screen.getByTestId("step-set-proxy")).toHaveAttribute("data-status", "active");
});

test("Stop routing invokes unset_proxy after the proxy was set (W5)", async () => {
  onMacHost();
  window.__TRAWLER_PROXY__ = "127.0.0.1:7999";
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={AT_SET_PROXY} />);

  fireEvent.click(await screen.findByRole("button", { name: "set system proxy" }));
  fireEvent.click(await screen.findByRole("button", { name: "stop routing through Trawler" }));

  await screen.findByText(/set it again to resume/);
  expect(invoke).toHaveBeenCalledWith("unset_proxy");
});

test("a connection event turns the proxy step green while trust warns — no flow, no decrypt", async () => {
  onMacHost();
  const { api, fireConnection } = mockApi(() => []); // no flows ever
  render(<App api={api} initialState={AT_SET_PROXY} />);
  await screen.findByTestId("step-set-proxy");

  fireConnection(); // a tunnel reached the proxy; its handshake failed → no flow
  await waitFor(() =>
    expect(screen.getByTestId("step-set-proxy-status")).toHaveTextContent(/receiving traffic/),
  );
  expect(screen.getByTestId("step-set-proxy")).toHaveAttribute("data-tone", "ok");
  // Trust can't be green (nothing decrypted); it points at the sending device.
  expect(screen.getByTestId("step-trust-ca-status")).toHaveTextContent(/arriving but not decrypting/);
  // Probe-green/traffic alone never reveals the list — that's still capture-only.
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

// Choosing a host offered by the dropdown, rather than typing one: the options
// are every host capture has seen, so narrowing to one excludes the others.
async function chooseHost(label: string) {
  fireEvent.click(screen.getByLabelText("host"));
  fireEvent.click(await screen.findByRole("option", { name: label }));
}

test("choosing a host narrows the list to it; SSE appends a new row once cleared", async () => {
  let rows = [
    flow({ id: "f1", uri: "http://example.com/" }),
    flow({ id: "f2", uri: "http://other.test/" }),
  ];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} initialState={PROVEN} />);
  await waitFor(() => expect(screen.getAllByTestId("flow-row")).toHaveLength(2));

  await chooseHost("other.test");
  await waitFor(() => expect(screen.getAllByTestId("flow-row")).toHaveLength(1));
  expect(screen.getByTestId("flow-row")).toHaveTextContent("other.test");

  await chooseHost("any host");
  rows = [...rows, flow({ id: "f3", uri: "http://example.com/second", status: 404 })];
  fire();
  await waitFor(() => expect(screen.getAllByTestId("flow-row")).toHaveLength(3));
});

test("the method filter offers the verbs as a closed set rather than free text", async () => {
  const { api } = mockApi(() => [flow({ id: "f1" })]);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");

  fireEvent.click(screen.getByLabelText("method"));
  const offered = (await screen.findAllByRole("option")).map((o) => o.textContent);
  expect(offered).toContain("GET");
  expect(offered).toContain("DELETE");
  // The exactness the design flagged: typing "GE" used to return an empty list
  // indistinguishable from no traffic, because the server matches the verb whole.
  expect(offered).not.toContain("GE");
});

test("host options are every host seen, not only those the current filter leaves", async () => {
  const rows = [
    flow({ id: "f1", uri: "http://example.com/" }),
    flow({ id: "f2", uri: "http://other.test/" }),
  ];
  const { api } = mockApi(() => rows);
  render(<App api={api} initialState={PROVEN} />);
  await waitFor(() => expect(screen.getAllByTestId("flow-row")).toHaveLength(2));

  await chooseHost("other.test");
  await waitFor(() => expect(screen.getAllByTestId("flow-row")).toHaveLength(1));

  // example.com is absent from the filtered list, and must still be selectable —
  // otherwise choosing a host would strand the user on it.
  fireEvent.click(screen.getByLabelText("host"));
  const offered = (await screen.findAllByRole("option")).map((o) => o.textContent);
  expect(offered).toContain("example.com");
  expect(offered).toContain("any host");
});

test("the status filter stays free text, deferred rather than half-converted", async () => {
  const { api } = mockApi(() => [flow({ id: "f1" })]);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");

  const status = screen.getByLabelText("status");
  expect(status.tagName).toBe("INPUT");
  fireEvent.change(status, { target: { value: "404" } });
  expect(status).toHaveValue("404");
});

test("a host that matches nothing says the query excludes the traffic, not that none exists", async () => {
  // The host is real (it was captured, so it is offered) but the traffic it
  // matched has since been filtered away by a second key.
  const { api } = mockApi(() => [flow({ id: "f1", uri: "http://example.com/" })]);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");

  fireEvent.change(screen.getByLabelText("filter"), { target: { value: "no-such-path" } });
  await waitFor(() => expect(screen.queryByTestId("flow-row")).toBeNull());
  expect(screen.getByTestId("empty")).toHaveTextContent("no flows match this filter");
  expect(screen.getByTestId("empty")).not.toHaveTextContent("no flows yet");
});

test("a proven launch with no traffic says nothing is captured, not that a filter excluded it", async () => {
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={PROVEN} />);

  const empty = await screen.findByTestId("empty");
  expect(empty).toHaveTextContent("no flows yet");
  expect(empty).not.toHaveTextContent("no flows match this filter");
});

test("the flow count reads as a value at a deliberately absent tone, not as a label", async () => {
  const { api } = mockApi(() => [flow({ id: "f1" }), flow({ id: "f2" })]);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findAllByTestId("flow-row");

  const count = screen.getByText(/2 flows/);
  expect(count.className).toMatch(/text-ink-mute/);
  expect(count.className).toMatch(/font-mono/);
});

test("the keyboard selects a row and opens its detail without a pointer", async () => {
  const { api } = mockApi(() => [
    flow({ id: "f1", uri: "http://example.com/a" }),
    flow({ id: "f2", uri: "http://example.com/b" }),
  ]);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findAllByTestId("flow-row");

  const list = screen.getByRole("listbox");
  fireEvent.keyDown(list, { key: "ArrowDown" });
  fireEvent.keyDown(list, { key: "ArrowDown" });
  fireEvent.keyDown(list, { key: "Enter" });

  await screen.findByTestId("detail");
  expect(api.getFlow).toHaveBeenCalledWith("f2");
});

// Inspector tests (E-UIINSPECT) run in the proven phase — post-onboarding, the
// list is where a flow is selected. Onboarding's StatusDot replaced the old
// LiveDot, so that dot's connection-state test retired with it.
test("clicking a row fetches /flows/{id} and opens the detail panel with curl", async () => {
  const { api } = mockApi(() => [flow({ id: "f1", uri: "http://example.com/a", method: "GET" })]);
  render(<App api={api} initialState={PROVEN} />);
  fireEvent.click(await screen.findByTestId("flow-row"));

  const detail = await screen.findByTestId("detail");
  expect(api.getFlow).toHaveBeenCalledWith("f1");
  expect(detail).toHaveTextContent("http://example.com/a");
  expect(screen.getByTestId("curl")).toHaveTextContent(
    "curl 'http://example.com/a' -H 'Accept: application/json'",
  );
});

test("a captured response body is fetched decoded and rendered by the viewer", async () => {
  const { api } = mockApi(() => [flow({ id: "f1" })]);
  (api.getFlow as ReturnType<typeof vi.fn>).mockResolvedValue({
    ...flow({ id: "f1" }),
    request_http_version: "HTTP/1.1",
    response_http_version: "HTTP/1.1",
    request_headers: [],
    response_headers: [],
    request_body: { state: "empty" },
    response_body: { state: "captured", size: 20, data_base64: "" },
  } satisfies FlowView);
  render(<App api={api} initialState={PROVEN} />);
  fireEvent.click(await screen.findByTestId("flow-row"));

  const tree = await screen.findByTestId("body-json");
  expect(api.getBody).toHaveBeenCalledWith("f1", "response", true);
  expect(tree).toHaveTextContent("greeting");
  expect(screen.getByRole("button", { name: "save" })).toBeInTheDocument();
});

test("selecting a different flow resets the copy button (no stale 'copied')", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  const rows = [flow({ id: "f1", uri: "http://a/" }), flow({ id: "f2", uri: "http://b/" })];
  const { api } = mockApi(() => rows);
  render(<App api={api} initialState={PROVEN} />);

  const [rowA, rowB] = await screen.findAllByTestId("flow-row");
  fireEvent.click(rowA);
  await screen.findByTestId("curl");
  fireEvent.click(screen.getByText("copy"));
  await waitFor(() => expect(screen.getByText("copied")).toBeInTheDocument());

  fireEvent.click(rowB);
  await waitFor(() => expect(screen.getByTestId("detail")).toHaveTextContent("http://b/"));
  expect(screen.getByText("copy")).toBeInTheDocument();
  expect(screen.queryByText("copied")).toBeNull();
  vi.unstubAllGlobals();
});
