import { render, screen, waitFor, fireEvent, act, within } from "@testing-library/react";
import { afterEach, beforeAll, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import { ApiError, apiAddr } from "./api";
import type { Filter, FlowSummary, FlowView, StreamHandlers, TrawlerApi } from "./api";
import { apiStub } from "./apiStub";
import { CAPTURE_COPY } from "./CaptureControls";
import * as files from "./body";
import type { OnbState } from "./onboardingMachine";
import { CA_FILENAME, guidedSteps } from "./setup";
import { TUNNEL_GRACE_MS } from "./verdicts";

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
    source: "captured",
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
  const api: TrawlerApi = apiStub({
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
    // The three doors answer what the server answers and nothing more: the list
    // refills from the bus, so a test must fire an event to see rows move —
    // which is exactly the dependency the screen has on the real system.
    exportHar: vi.fn(async () => ({ bytes: new TextEncoder().encode("{}"), filename: "trawler-x.har" })),
    importHar: vi.fn(async () => 1),
    importCurl: vi.fn(async () => 1),
    clearFlows: vi.fn(async () => 1),
    stream: vi.fn((handlers: StreamHandlers) => {
      onEvent = handlers.onEvent;
      onConnection = handlers.onConnection ?? (() => {});
      return () => {};
    }),
  });
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

// A tunnel is a live signal about traffic, so it lands on step 3 — first as a
// connection in its grace, then, once no flow has answered it, as the warning
// that names the missing certificate. The probed steps never hear about it.
test("a connection event reaches step 3, not the proxy step — and warns once no flow answers it", async () => {
  onMacHost();
  const { api, fireConnection } = mockApi(() => []); // no flows ever
  render(<App api={api} initialState={AT_SET_PROXY} />);
  await screen.findByTestId("step-set-proxy");

  fireConnection(); // a tunnel reached the proxy; its handshake failed → no flow
  await waitFor(() =>
    expect(screen.getByTestId("step-verify-status")).toHaveTextContent("a connection just arrived…"),
  );
  expect(screen.getByTestId("step-set-proxy-status")).toHaveTextContent("can't check automatically");
  expect(screen.getByTestId("step-set-proxy")).not.toHaveAttribute("data-tone", "ok");

  // The screen's clock ticks on; once the grace is spent the tunnel is a failure.
  const realNow = Date.now;
  vi.spyOn(Date, "now").mockImplementation(() => realNow() + TUNNEL_GRACE_MS + 1_000);
  await waitFor(
    () => expect(screen.getByTestId("step-verify-status")).toHaveTextContent("arriving but not decrypting"),
    { timeout: 3_000 },
  );
  expect(screen.getByTestId("step-verify")).toHaveAttribute("data-tone", "warn");
  expect(screen.getByTestId("step-trust-ca")).not.toHaveAttribute("data-tone", "warn");
  // Traffic alone never reveals the list — that's still capture-only.
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

// The probes are a snapshot of this machine. Two things re-take it without an
// action: the control above the steps, and the window coming back into focus —
// which is when the user has just been in System Settings.
test("check again and window focus each re-read both probes, and the reading is stamped", async () => {
  onMacHost();
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={ONBOARDING} />);
  await waitFor(() => expect(screen.getByTestId("recheck")).toHaveTextContent(/checked at \d\d:\d\d:\d\d/));
  const probes = () => vi.mocked(invoke).mock.calls.filter(([name]) => name === "ca_trusted" || name === "proxy_status").length;
  expect(probes()).toBe(2);

  fireEvent.click(screen.getByRole("button", { name: "check again" }));
  await waitFor(() => expect(probes()).toBe(4));

  fireEvent(window, new Event("focus"));
  await waitFor(() => expect(probes()).toBe(6));
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

test("the status filter stays free text, because the useful set is unknowable", async () => {
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
test("clicking a row fetches /flows/{id} and opens the detail panel with copy-as-curl", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  const { api } = mockApi(() => [flow({ id: "f1", uri: "http://example.com/a", method: "GET" })]);
  render(<App api={api} initialState={PROVEN} />);
  fireEvent.click(await screen.findByTestId("flow-row"));

  const detail = await screen.findByTestId("detail");
  expect(api.getFlow).toHaveBeenCalledWith("f1");
  expect(detail).toHaveTextContent("http://example.com/a");
  expect(detail.textContent).not.toContain("curl '");
  fireEvent.click(screen.getByRole("button", { name: "copy as curl" }));
  expect(writeText).toHaveBeenCalledWith("curl 'http://example.com/a' -H 'Accept: application/json'");
  vi.unstubAllGlobals();
});

test("Esc closes the panel and hands focus back to the list that opened it", async () => {
  const { api } = mockApi(() => [flow({ id: "f1", uri: "http://example.com/a" })]);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findAllByTestId("flow-row");

  const list = screen.getByRole("listbox");
  list.focus();
  fireEvent.keyDown(list, { key: "ArrowDown" });
  fireEvent.keyDown(list, { key: "Enter" });
  await screen.findByTestId("detail");

  fireEvent.keyDown(document.body, { key: "Escape" });
  await waitFor(() => expect(screen.queryByTestId("detail")).toBeNull());
  expect(document.activeElement).toBe(list);
});

test("a captured response body is fetched pretty and rendered by the viewer", async () => {
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
  await screen.findByTestId("detail");
  // The panel opens on the request leg (DEC-31); the response body is one act away.
  fireEvent.click(within(screen.getByRole("radiogroup", { name: "leg" })).getByRole("radio", { name: "response" }));

  const tree = await screen.findByTestId("body-json");
  expect(api.getBody).toHaveBeenCalledWith("f1", "response", true);
  expect(tree).toHaveTextContent("greeting");
  expect(screen.getByRole("button", { name: "save" })).toBeInTheDocument();
  // The label and the fetch asserted together: a switch reading "pretty" over a
  // source fetch would satisfy either one alone.
  const view = within(screen.getByRole("radiogroup", { name: "response body mode" }));
  expect(view.getByRole("radio", { name: "pretty" })).toBeChecked();
  expect(view.getByRole("radio", { name: "raw" })).toBeInTheDocument();
  expect(view.queryByRole("radio", { name: "decoded" })).toBeNull();
});

test("selecting a different flow resets the copy button (no stale 'copied')", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  const rows = [flow({ id: "f1", uri: "http://a/" }), flow({ id: "f2", uri: "http://b/" })];
  const { api } = mockApi(() => rows);
  render(<App api={api} initialState={PROVEN} />);

  const [rowA, rowB] = await screen.findAllByTestId("flow-row");
  fireEvent.click(rowA);
  await screen.findByTestId("detail");
  fireEvent.click(screen.getByRole("button", { name: "copy as curl" }));
  await waitFor(() => expect(screen.getByText("copied")).toBeInTheDocument());

  fireEvent.click(rowB);
  await waitFor(() => expect(screen.getByTestId("detail")).toHaveTextContent("http://b/"));
  expect(screen.getByRole("button", { name: "copy as curl" })).toBeInTheDocument();
  expect(screen.queryByText("copied")).toBeNull();
  vi.unstubAllGlobals();
});

// UC-4 amended (E-EXPORT.W6): the flow list's first controls that change server
// state. The bounded contract is the whole surface — the acts reached from the
// real header beside the real filters, the list refilling from the bus rather
// than from a mutation's own response, and the absence that only a clear can
// produce.
const captureTrigger = () => screen.getByRole("button", { name: CAPTURE_COPY.trigger });
const clearCapture = (count: number) => fireEvent.click(screen.getByRole("button", { name: `clear ${count} ${count === 1 ? "flow" : "flows"}` }));
async function chooseAct(label: string) {
  fireEvent.click(captureTrigger());
  fireEvent.click(await screen.findByRole("menuitem", { name: label }));
}

test("the clear shortcut and transfer menu sit past the filters", async () => {
  const { api } = mockApi(() => [flow()]);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");

  const header = document.querySelector("header") as HTMLElement;
  const controls = [...header.querySelectorAll("button, input")];
  // The trailing edge: everything the user narrows the list with comes first,
  // then clear, then the transfer menu.
  expect(controls.indexOf(screen.getByRole("button", { name: "clear 1 flow" }))).toBe(controls.length - 2);
  expect(controls.indexOf(captureTrigger())).toBe(controls.length - 1);
  expect(screen.queryByRole("menu")).toBeNull();
  expect(within(header).getByTestId("capture-slot")).toHaveTextContent("1 flow");
});

test("export is scoped by whatever the filters selected, not by the whole capture", async () => {
  const rows = [flow({ id: "f1", uri: "http://example.com/" }), flow({ id: "f2", uri: "http://other.test/" })];
  const { api } = mockApi(() => rows);
  vi.spyOn(files, "saveBody").mockResolvedValue("/tmp/x.har");
  render(<App api={api} initialState={PROVEN} />);
  await waitFor(() => expect(screen.getAllByTestId("flow-row")).toHaveLength(2));

  await chooseHost("other.test");
  await waitFor(() => expect(screen.getAllByTestId("flow-row")).toHaveLength(1));

  await chooseAct(CAPTURE_COPY.exportAct);
  await waitFor(() => expect(api.exportHar).toHaveBeenCalledWith({ host: "other.test" }));
});

test("clearing empties the list into an absence only a clear produces, and the count follows", async () => {
  let rows = [flow({ id: "f1" }), flow({ id: "f2" })];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} initialState={PROVEN} />);
  await waitFor(() => expect(screen.getAllByTestId("flow-row")).toHaveLength(2));

  clearCapture(2);
  expect(await screen.findByRole("dialog")).toHaveAttribute("aria-label", "clear 2 flows?");
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.clear }));
  await waitFor(() => expect(api.clearFlows).toHaveBeenCalled());

  // The server announces the clear and the list refills from the bus — the
  // response carries a count, not rows, so it could not refill it anyway.
  rows = [];
  fire();
  await waitFor(() => expect(screen.getByTestId("empty")).toHaveTextContent("capture cleared"));
  expect(screen.getByTestId("capture-slot")).toHaveTextContent("0 flows");
});

test("the cleared absence gives way as soon as rows come back", async () => {
  let rows = [flow({ id: "f1" })];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");

  clearCapture(1);
  await screen.findByRole("dialog");
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.clear }));
  rows = [];
  fire();
  await waitFor(() => expect(screen.getByTestId("empty")).toHaveTextContent("capture cleared"));

  rows = [flow({ id: "f2" })];
  fire();
  await waitFor(() => expect(screen.getByTestId("flow-row")).toBeInTheDocument());

  rows = [];
  fire();
  // Emptied by eviction this time, not by a clear — so it is not that absence.
  await waitFor(() => expect(screen.getByTestId("empty")).toHaveTextContent("no flows yet"));
});

test("narrowing after a clear reads as the filter's absence, not as the clear's", async () => {
  let rows = [flow({ id: "f1", uri: "http://example.com/" })];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");

  clearCapture(1);
  await screen.findByRole("dialog");
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.clear }));
  rows = [];
  fire();
  await waitFor(() => expect(screen.getByTestId("empty")).toHaveTextContent("capture cleared"));

  fireEvent.change(screen.getByLabelText("filter"), { target: { value: "nothing" } });
  await waitFor(() => expect(screen.getByTestId("empty")).toHaveTextContent("no flows match this filter"));
});

test("a dismissed clear ask sends the live filter and refills from the bus without a dialog", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => command === "notice_dismissed");
  let rows = [flow({ id: "f1", uri: "http://example.com/" }), flow({ id: "f2", uri: "http://other.test/" })];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} initialState={PROVEN} />);
  await waitFor(() => expect(screen.getAllByTestId("flow-row")).toHaveLength(2));

  await chooseHost("other.test");
  await waitFor(() => expect(screen.getAllByTestId("flow-row")).toHaveLength(1));
  clearCapture(1);

  await waitFor(() => expect(api.clearFlows).toHaveBeenCalledWith({ host: "other.test" }));
  expect(screen.queryByRole("dialog")).toBeNull();

  rows = [flow({ id: "f1", uri: "http://example.com/" })];
  fire();
  await waitFor(() => expect(screen.getByTestId("empty")).toHaveTextContent("capture cleared"));
});

test("an imported file lands as marked rows the list gets from the bus", async () => {
  let rows: FlowSummary[] = [];
  const { api, fire } = mockApi(() => rows);
  vi.spyOn(files, "openFile").mockResolvedValue({
    name: "session.har",
    bytes: new TextEncoder().encode('{"log":{"entries":[]}}'),
  });
  render(<App api={api} initialState={PROVEN} />);
  await waitFor(() => expect(screen.getByTestId("empty")).toBeInTheDocument());

  await chooseAct(CAPTURE_COPY.importName);
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.choose }));
  await waitFor(() => expect(api.importHar).toHaveBeenCalled());
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

  rows = [flow({ id: "i1", source: "imported" })];
  fire();
  await waitFor(() => expect(screen.getByLabelText("imported")).toBeInTheDocument());
});

test("a pasted command lands as a request-only row, and a refusal keeps the paste on screen", async () => {
  let rows: FlowSummary[] = [];
  const { api, fire } = mockApi(() => rows);
  (api.importCurl as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
    new Error("unsupported flag --data-urlencode"),
  );
  render(<App api={api} initialState={PROVEN} />);
  await waitFor(() => expect(screen.getByTestId("empty")).toBeInTheDocument());

  await chooseAct(CAPTURE_COPY.importName);
  const paste = screen.getByRole("textbox", { name: CAPTURE_COPY.pasteLabel });
  fireEvent.change(paste, { target: { value: "curl --data-urlencode 'q=1' http://x/" } });
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.import }));

  expect(await screen.findByTestId("import-failed")).toHaveTextContent("unsupported flag --data-urlencode");
  expect(screen.getByTestId("empty")).toBeInTheDocument(); // nothing was stored

  fireEvent.change(paste, { target: { value: "curl http://x/" } });
  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.import }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

  rows = [flow({ id: "p1", source: "imported", status: null, duration_ms: null })];
  fire();
  await waitFor(() => expect(screen.getByLabelText("imported")).toBeInTheDocument());
});

// The shell's connection states, driven through a core whose answer the test
// controls: the whole point is what the SCREEN does when the API stops answering
// or refuses, not what the hook holds.
function shellApi(answer: () => Promise<FlowSummary[]>) {
  const listFlows = vi.fn((_filter: Filter) => answer());
  const api: TrawlerApi = apiStub({
    listFlows,
    caCertPem: async () => PEM,
    getFlow: async (id: string): Promise<FlowView> => ({
      ...flow({ id }),
      request_http_version: "HTTP/1.1",
      response_http_version: "HTTP/1.1",
      request_headers: [],
      response_headers: [],
      request_body: { state: "empty" },
      response_body: { state: "empty" },
    }),
  });
  return { api, listFlows };
}

const HTTPS = flow({ uri: "https://example.com/" });

test("the status filter says what it takes, and hints no keypad it cannot honour", async () => {
  const { api } = shellApi(async () => [HTTPS]);
  render(<App api={api} initialState={PROVEN} />);

  const status = await screen.findByLabelText("status");
  expect(status).toHaveAttribute("placeholder", "status — 404 or 4xx");
  // A class is a letter; a numeric keypad is a promise the field can no longer keep.
  expect(status).not.toHaveAttribute("inputMode");
  expect(status.className).toContain("w-field");
  expect(status.className).not.toContain("w-field-sm");
});

test("a status class reaches the API as typed", async () => {
  const { api, listFlows } = shellApi(async () => [HTTPS]);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");

  fireEvent.change(screen.getByLabelText("status"), { target: { value: "4xx" } });
  await waitFor(() => expect(listFlows).toHaveBeenLastCalledWith({ status: "4xx" }));
});

test("a core that stops answering states it above rows that do not move", async () => {
  let answer: () => Promise<FlowSummary[]> = async () => [HTTPS];
  const { api } = shellApi(() => answer());
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");

  answer = () => Promise.reject(new TypeError("Failed to fetch"));
  fireEvent.change(screen.getByLabelText("filter"), { target: { value: "x" } });

  await screen.findByText("trawler is not answering");
  expect(screen.getByText(new RegExp(`no answer from ${apiAddr()}`))).toBeInTheDocument();
  expect(screen.getByText(/the rows below are the last it sent/)).toBeInTheDocument();
  // The rows were true when they arrived; the band qualifies them, it does not
  // delete them — and the count still counts what is on screen.
  expect(screen.getAllByTestId("flow-row")).toHaveLength(1);
  expect(screen.getByText("1 flow")).toBeInTheDocument();
  expect(screen.queryByTestId("empty")).toBeNull();
});

test("with nothing captured the same sentence stands in the row region, and no band", async () => {
  const { api } = shellApi(() => Promise.reject(new TypeError("Failed to fetch")));
  render(<App api={api} initialState={PROVEN} />);

  await screen.findByText("trawler is not answering");
  const region = screen.getByTestId("empty");
  expect(region).toHaveTextContent("trawler is not answering");
  expect(region).toHaveTextContent("nothing has reached the window");
  // Said once: the statement sits immediately above what it is about, and with
  // no rows there is nothing above to qualify.
  expect(screen.getAllByText("trawler is not answering")).toHaveLength(1);
  expect(screen.queryByText(/the rows below are the last it sent/)).toBeNull();
});

test("a refused query is not a failed connection — the row region says so, the band stays away", async () => {
  let answer: () => Promise<FlowSummary[]> = async () => [HTTPS];
  const { api } = shellApi(() => answer());
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");

  answer = () => Promise.reject(new ApiError(400, "GET /flows failed: 400"));
  fireEvent.change(screen.getByLabelText("status"), { target: { value: "4xz" } });

  const region = await screen.findByTestId("empty");
  expect(region).toHaveTextContent("4xz is not a status");
  expect(region).toHaveTextContent("type a code (404) or a class (4xx)");
  expect(screen.queryByText("trawler is not answering")).toBeNull();
  // The field keeps the text — nothing is corrected for the user.
  expect(screen.getByLabelText("status")).toHaveValue("4xz");
  expect(screen.getByText("0 flows")).toBeInTheDocument();
});

test("retry disables its button and names what is awaited, then leaves with focus placed", async () => {
  let release: (rows: FlowSummary[]) => void = () => {};
  let answer: () => Promise<FlowSummary[]> = async () => [HTTPS];
  const { api } = shellApi(() => answer());
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");

  answer = () => Promise.reject(new TypeError("Failed to fetch"));
  fireEvent.change(screen.getByLabelText("filter"), { target: { value: "x" } });
  await screen.findByText("trawler is not answering");

  answer = () => new Promise<FlowSummary[]>((res) => (release = res));
  fireEvent.click(screen.getByRole("button", { name: "retry" }));

  // Fork D4: both halves or neither.
  await waitFor(() => expect(screen.getByRole("button", { name: "retry" })).toBeDisabled());
  expect(screen.getByText(`trying ${apiAddr()}…`)).toBeInTheDocument();

  await act(async () => {
    release([HTTPS]);
  });
  await waitFor(() => expect(screen.queryByText("trawler is not answering")).toBeNull());
  // A surface that removes itself may not leave focus on <body>.
  expect(document.activeElement).toBe(screen.getByRole("main"));
});

test("a retry that fails again keeps the band and re-enables the way out", async () => {
  let answer: () => Promise<FlowSummary[]> = async () => [HTTPS];
  const { api } = shellApi(() => answer());
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");

  answer = () => Promise.reject(new TypeError("Failed to fetch"));
  fireEvent.change(screen.getByLabelText("filter"), { target: { value: "x" } });
  await screen.findByText("trawler is not answering");

  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "retry" }));
  });
  expect(screen.getByText("trawler is not answering")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "retry" })).toBeEnabled();
  expect(document.activeElement).not.toBe(screen.getByRole("main"));
});

// The band belongs to the flow shell. Onboarding has its own account of what is
// and is not working, and this epic does not open it.
test("the onboarding phase renders no connection band", async () => {
  const { api } = shellApi(() => Promise.reject(new TypeError("Failed to fetch")));
  render(<App api={api} initialState={ONBOARDING} />);
  await screen.findByText(/trust the CA/);
  expect(screen.queryByText("trawler is not answering")).toBeNull();
});

// Regression (review round 1, correctness + altitude): the pane's failure and the
// list's are two independent facts, and folding them into one boolean let a
// latched pane failure speak for the list. Both tests fail before the fix.
function paneApi(listAnswer: () => Promise<FlowSummary[]>, flowAnswer: () => Promise<FlowView>) {
  const api: TrawlerApi = apiStub({
    listFlows: () => listAnswer(),
    caCertPem: async () => PEM,
    getFlow: () => flowAnswer(),
  });
  return api;
}

test("a failed detail fetch does not speak for the list — a refused query keeps its own sentence", async () => {
  let list: () => Promise<FlowSummary[]> = async () => [HTTPS];
  const api = paneApi(
    () => list(),
    () => Promise.reject(new ApiError(500, "GET /flows/f1 failed: 500")),
  );
  render(<App api={api} initialState={PROVEN} />);
  fireEvent.click(await screen.findByTestId("flow-row"));
  await waitFor(() => expect(screen.getByTestId("detail-loading")).toBeInTheDocument());

  // The core is plainly alive: it answers the listing, and it answered the pane
  // with a 500. Now refuse the query.
  list = () => Promise.reject(new ApiError(400, "GET /flows failed: 400"));
  fireEvent.change(screen.getByLabelText("status"), { target: { value: "4xz" } });

  const region = await screen.findByTestId("empty");
  expect(region).toHaveTextContent("4xz is not a status");
  expect(screen.queryByText("trawler is not answering")).toBeNull();
});

test("a pane failure never raises the connection band on a core that is answering", async () => {
  const api = paneApi(async () => [HTTPS], () => Promise.reject(new TypeError("Failed to fetch")));
  render(<App api={api} initialState={PROVEN} />);
  fireEvent.click(await screen.findByTestId("flow-row"));
  await waitFor(() =>
    expect(screen.getByTestId("detail-failed")).toHaveTextContent("could not load this flow"),
  );

  // The listing keeps arriving, so the core is answering. One flow's fetch
  // failing is a fact about that flow, and the band speaks only for the
  // connection — otherwise it claims an outage that is not happening.
  expect(screen.queryByTestId("band")).toBeNull();
  expect(screen.queryByText("trawler is not answering")).toBeNull();
  expect(screen.getAllByTestId("flow-row")).toHaveLength(1);
});

// Regression (review round 2, correctness): round 1's fix cleared the pane's
// latched failure when a listing arrived — without re-fetching the flow. The
// band went away and the pane was left on "loading this flow…" forever, with
// nothing on screen saying otherwise. A zone may not be silent about its own
// failure just because another zone stopped talking.
test("a pane whose fetch failed says so, and does not keep claiming it is loading", async () => {
  const api = paneApi(async () => [HTTPS], () => Promise.reject(new TypeError("Failed to fetch")));
  render(<App api={api} initialState={PROVEN} />);
  fireEvent.click(await screen.findByTestId("flow-row"));

  await waitFor(() => expect(screen.queryByTestId("detail-loading")).toBeNull());
  expect(screen.getByTestId("detail-failed")).toHaveTextContent("could not load this flow");

  // An unrelated listing arriving must not silence it either.
  fireEvent.change(screen.getByLabelText("filter"), { target: { value: "example" } });
  await waitFor(() => expect(screen.getByLabelText("filter")).toHaveValue("example"));
  expect(screen.getByTestId("detail-failed")).toHaveTextContent("could not load this flow");
});

// Regression (review round 2, correctness): with the list empty for its own
// reason, the band was gated off (no rows) and the row region only ever looked
// at the list's failure — so a failed pane fetch was stated by nobody at all.
test("a pane failure is stated even when the list is empty for its own reason", async () => {
  let list: () => Promise<FlowSummary[]> = async () => [HTTPS];
  const api = paneApi(
    () => list(),
    () => Promise.reject(new TypeError("Failed to fetch")),
  );
  render(<App api={api} initialState={PROVEN} />);
  fireEvent.click(await screen.findByTestId("flow-row"));
  await waitFor(() =>
    expect(screen.getByTestId("detail-failed")).toHaveTextContent("could not load this flow"),
  );

  list = async () => [];
  fireEvent.change(screen.getByLabelText("filter"), { target: { value: "matches-nothing" } });
  await screen.findByTestId("empty");

  // The list says its own thing; the pane still says its own.
  expect(screen.getByTestId("empty")).toHaveTextContent("no flows match this filter");
  expect(screen.getByTestId("detail-failed")).toHaveTextContent("could not load this flow");
});

// Regression (review rounds 3 and 4): the pane must not repeat the connection's
// alarm, and must not go back to claiming it is loading a fetch that already
// came back. It does neither — it states its own fact, quietly, and the band
// keeps the only ⚠ on screen.
test("a pane failure states itself without repeating the connection's alarm", async () => {
  const api = paneApi(
    () => Promise.reject(new TypeError("Failed to fetch")),
    () => Promise.reject(new TypeError("Failed to fetch")),
  );
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByText("trawler is not answering");

  expect(screen.getAllByText("trawler is not answering")).toHaveLength(1);
  expect(document.querySelectorAll('[data-tone="warn"]')).toHaveLength(1);
});

// Round 4's blocker: coupling the pane's sentence to the connection meant a core
// going silent AFTER the pane had already failed silently un-said the pane's
// failure, putting "loading this flow…" back over a fetch that had come back.
test("a core going silent does not put the pane back on 'loading this flow…'", async () => {
  let list: () => Promise<FlowSummary[]> = async () => [HTTPS];
  const api = paneApi(
    () => list(),
    () => Promise.reject(new ApiError(500, "GET /flows/f1 failed: 500")),
  );
  render(<App api={api} initialState={PROVEN} />);
  fireEvent.click(await screen.findByTestId("flow-row"));
  await waitFor(() =>
    expect(screen.getByTestId("detail-failed")).toHaveTextContent("could not load this flow"),
  );

  list = () => Promise.reject(new TypeError("Failed to fetch"));
  fireEvent.change(screen.getByLabelText("filter"), { target: { value: "example" } });
  await screen.findByText("trawler is not answering");

  expect(screen.queryByTestId("detail-loading")).toBeNull();
  expect(screen.getByTestId("detail-failed")).toHaveTextContent("could not load this flow");
});

// The focus move belongs to the retry, not to every recovery: a core that comes
// back on its own must not yank focus out from under someone who is typing.
test("a connection that recovers on its own does not move focus", async () => {
  let answer: () => Promise<FlowSummary[]> = () => Promise.reject(new TypeError("dead"));
  const { api } = shellApi(() => answer());
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByText("trawler is not answering");

  const field = screen.getByLabelText("filter");
  field.focus();
  answer = async () => [HTTPS];
  fireEvent.change(field, { target: { value: "example" } });

  await waitFor(() => expect(screen.queryByText("trawler is not answering")).toBeNull());
  expect(document.activeElement).toBe(field);
});

// Regression (review round 5): the run ranked "not answering" above the two
// empties and never against a refusal. Ranking them anyway hid a real answer the
// core had given — both are true, so the shell states both.
test("a core that goes silent after refusing a query states both, hiding neither", async () => {
  let list: () => Promise<FlowSummary[]> = async () => [HTTPS];
  const { api } = shellApi(() => list());
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");

  list = () => Promise.reject(new ApiError(400, "GET /flows failed: 400"));
  fireEvent.change(screen.getByLabelText("status"), { target: { value: "4xz" } });
  await waitFor(() => expect(screen.getByTestId("empty")).toHaveTextContent("4xz is not a status"));

  list = () => Promise.reject(new TypeError("Failed to fetch"));
  fireEvent.change(screen.getByLabelText("filter"), { target: { value: "x" } });

  const band = await screen.findByTestId("band");
  expect(band).toHaveTextContent("trawler is not answering");
  // The refusal keeps its own region; the silence is stated above it.
  expect(screen.getByTestId("empty")).toHaveTextContent("4xz is not a status");
  // And the band makes no claim about rows that are not there.
  expect(band).not.toHaveTextContent("the rows below are the last it sent");});
