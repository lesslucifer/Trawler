import { render, screen, waitFor, fireEvent, act, within } from "@testing-library/react";
import { afterEach, beforeAll, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import { ApiError, apiAddr, UNREACHABLE } from "./api";
import type { Filter, FlowSummary, FlowView, StreamHandlers, TrawlerApi } from "./api";
import { apiStub } from "./apiStub";
import { CAPTURE_COPY } from "./CaptureControls";
import * as files from "./body";
import type { OnbState } from "./onboardingMachine";
import { CA_FILENAME } from "./setup";
import { TUNNEL_GRACE_MS } from "./verdicts";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => {}) }));
// App wires a quit-time close guard (W5); stub the window so onCloseRequested is a
// no-op here. The guard's own behavior is covered in closeConfirm.test.tsx.
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ onCloseRequested: async () => () => {}, destroy: async () => {} }),
}));

// Capture is proven only by a live decrypted flow (W4); until then App shows
// onboarding. These list/filter tests force the proven phase to reach the list.
const PROVEN: OnbState = { phase: "proven", step: "verify", everProven: true, since: null };
const ONBOARDING: OnbState = { phase: "onboarding", step: "trust-ca", everProven: false, since: null };

const QUESTION = "which device's traffic do you want to see";

// The setup screen's one question is a dropdown: open it, then take the answer.
const pickPlatform = (label: string) => {
  fireEvent.click(screen.getByRole("button", { name: QUESTION }));
  fireEvent.click(screen.getByRole("option", { name: label }));
};

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
    seed: null,
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
  expect(await screen.findByText("which device's traffic do you want to see")).toBeInTheDocument();
  expect(screen.queryByTestId("empty")).toBeNull();
});

test("launch with a pre-existing decrypted HTTPS flow goes straight to the list", async () => {
  const { api } = mockApi(() => [flow({ uri: "https://example.com/" })]);
  render(<App api={api} />);
  const row = await screen.findByTestId("flow-row");
  expect(row).toHaveTextContent("example.com");
  expect(screen.queryByText("which device's traffic do you want to see")).toBeNull();
});

test("launch with only a plain-HTTP flow shows onboarding (http:// is not proof)", async () => {
  const { api } = mockApi(() => [flow({ uri: "http://example.com/" })]);
  render(<App api={api} />);
  expect(await screen.findByText("which device's traffic do you want to see")).toBeInTheDocument();
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

// R3: E-EXPORT and E-REPLAY added rows no device routed, and the proof rule
// predated them. Only a captured row says anything about a device's setup.
test.each(["imported", "sent"] as const)(
  "a launch whose only https row was %s shows setup, the try task still waiting",
  async (source) => {
    const { api } = mockApi(() => [flow({ uri: "https://example.com/", source, started_at: new Date().toISOString() })]);
    render(<App api={api} />);
    expect(await screen.findByText(QUESTION)).toBeInTheDocument();
    expect(screen.getByTestId("task-3")).toHaveTextContent("waiting for the first request");
    expect(screen.queryByTestId("flow-row")).toBeNull();
  },
);

test("a request Trawler sent, arriving on the setup screen, neither proves setup nor reads as capturing", async () => {
  let rows: FlowSummary[] = [];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} />);
  await screen.findByText(QUESTION);

  rows = [flow({ uri: "https://example.com/", source: "sent", started_at: new Date().toISOString() })];
  fire();
  await waitFor(() => expect(vi.mocked(api.listFlows).mock.calls.length).toBeGreaterThan(1));
  expect(screen.getByText(QUESTION)).toBeInTheDocument();
  expect(screen.getByTestId("task-3")).toHaveTextContent("waiting for the first request");
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

test("a decrypted HTTPS flow arriving live flips Verify green and reveals the list", async () => {
  let rows: FlowSummary[] = [];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} />);
  await screen.findByText("which device's traffic do you want to see"); // fresh launch → onboarding

  rows = [flow({ uri: "https://example.com/" })];
  fire();
  const row = await screen.findByTestId("flow-row");
  expect(row).toHaveTextContent("example.com");
  expect(screen.queryByText("which device's traffic do you want to see")).toBeNull();
});

test("a plain-HTTP flow arriving live does not flip out of onboarding", async () => {
  let rows: FlowSummary[] = [];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} />);
  await screen.findByText("which device's traffic do you want to see");

  rows = [flow({ uri: "http://example.com/" })];
  fire();
  await waitFor(() => expect(vi.mocked(api.listFlows).mock.calls.length).toBeGreaterThan(1));
  expect(screen.getByText("which device's traffic do you want to see")).toBeInTheDocument();
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

test("a forced-proven phase reveals the flow list", async () => {
  const { api } = mockApi(() => [flow()]);
  render(<App api={api} initialState={PROVEN} />);
  const row = await screen.findByTestId("flow-row");
  expect(row).toHaveTextContent("example.com");
  expect(row.querySelector(".text-ok")).not.toBeNull();
  expect(screen.queryByText("which device's traffic do you want to see")).toBeNull();
});

test("tapping the status dot re-opens setup and stays there despite the captured flow", async () => {
  // A decrypted flow is still present, but re-invoke is a deliberate user action;
  // the live check must not instantly bounce an ever-proven user back to the list.
  const { api } = mockApi(() => [flow({ uri: "https://example.com/" })]);
  render(<App api={api} initialState={PROVEN} />);
  await screen.findByTestId("flow-row");
  fireEvent.click(screen.getByTestId("status-dot"));
  expect(await screen.findByText("which device's traffic do you want to see")).toBeInTheDocument();
  expect(screen.queryByTestId("flow-row")).toBeNull();
  expect(screen.getByRole("button", { name: /back to flows/ })).toBeInTheDocument(); // Back to flows, since ever proven
  fireEvent.click(screen.getByRole("button", { name: /back to flows/ }));
  expect(await screen.findByTestId("flow-row")).toBeInTheDocument();
});

// F27: the one thing most users want costs one prompt, because the shell runs
// both elevated commands under one authorization. The seam is that the screen
// asks for the PEM and the combined command in one act, and that a command
// exiting 0 still ends nothing — only a decrypted request does.
test("Set up this Mac fetches the PEM and invokes the one combined command", async () => {
  onMacHost();
  window.__TRAWLER_PROXY__ = "127.0.0.1:7999";
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={ONBOARDING} />);

  fireEvent.click(await screen.findByRole("button", { name: "Set up this Mac" }));

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_up_this_machine", {
      pem: PEM,
      host: "127.0.0.1",
      port: 7999,
    }),
  );
  expect(api.caCertPem).toHaveBeenCalledOnce();
  expect(vi.mocked(invoke).mock.calls.filter(([n]) => n === "set_proxy")).toHaveLength(0);
  expect(screen.getByText("which device's traffic do you want to see")).toBeInTheDocument();
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

  fireEvent.click(within(await screen.findByTestId("task-1")).getByText("do it manually"));
  fireEvent.click(screen.getByRole("button", { name: "save…" }));
  await screen.findByText(new RegExp(`saved as ${CA_FILENAME}`));

  expect(api.caCertPem).toHaveBeenCalledOnce();
  expect(saved.name).toBe(CA_FILENAME);
  expect(screen.getByTestId("steps")).toHaveTextContent(
    `Save Trawler's certificate as ${saved.name}`,
  );
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

// F18: the shell classifies its own failure, and the screen words the one the
// user caused. The seam is that `refused` survives the IPC boundary — a
// rejection is an arbitrary value, and a string would read as a crash.
test("a dismissed prompt crosses the boundary as a refusal, with the tool's text kept", async () => {
  onMacHost();
  rejectCommand("set_up_this_machine", {
    refused: true,
    detail: "0:36: execution error: User cancelled. (-128)",
  } as never);
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={ONBOARDING} />);

  fireEvent.click(await screen.findByRole("button", { name: "Set up this Mac" }));

  await screen.findByText("The password prompt was cancelled. Try again, or follow the manual steps.");
  fireEvent.click(screen.getByText("details"));
  expect(screen.getByText("0:36: execution error: User cancelled. (-128)")).toBeInTheDocument();
  expect(screen.getByTestId("task-1")).not.toHaveTextContent(/done — installed/);
});

test("a real failure keeps the OS's own words and leaves the manual route open", async () => {
  onMacHost();
  rejectCommand("set_up_this_machine", {
    refused: false,
    detail: "no network services to configure",
  } as never);
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={ONBOARDING} />);

  fireEvent.click(await screen.findByRole("button", { name: "Set up this Mac" }));
  await screen.findByText(/setting up failed/);

  const route = within(screen.getByTestId("task-1")).getByText("do it manually").closest("details")!;
  expect(route).toHaveAttribute("open");
  expect(route).toHaveTextContent(/security add-trusted-cert/);
  expect(screen.getByTestId("set-up-status")).toHaveTextContent("no network services to configure");
});

// The Simulator is the one platform whose primary action is the proxy alone:
// it follows this Mac's system proxy and keeps a certificate store of its own.
async function simulator(initial = ONBOARDING) {
  onMacHost();
  window.__TRAWLER_PROXY__ = "127.0.0.1:7999";
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={initial} />);
  await screen.findByText(QUESTION);
  pickPlatform("iOS Simulator");
  return api;
}

test("Set the system proxy invokes set_proxy with the configured address, and proves nothing", async () => {
  await simulator();

  fireEvent.click(screen.getByRole("button", { name: "Set the system proxy" }));

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("set_proxy", { host: "127.0.0.1", port: 7999 }),
  );
  expect(screen.getByText("which device's traffic do you want to see")).toBeInTheDocument();
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

test("a rejected set_proxy reports failed and keeps the manual route reachable", async () => {
  rejectCommand("set_proxy", { refused: false, detail: "no network services" } as never);
  await simulator();

  fireEvent.click(screen.getByRole("button", { name: "Set the system proxy" }));
  await screen.findByText(/setting the proxy failed/);

  fireEvent.click(within(screen.getByTestId("task-2")).getByText("do it manually"));
  expect(screen.getByTestId("task-2")).toHaveTextContent(/networksetup/);
});

test("Stop routing invokes unset_proxy after the proxy was set", async () => {
  await simulator();

  fireEvent.click(screen.getByRole("button", { name: "Set the system proxy" }));
  fireEvent.click(await screen.findByRole("button", { name: "stop routing through Trawler" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("unset_proxy"));
});

// A tunnel is a live signal about traffic, so it settles the task the traffic
// layer owns — first as a connection in its grace, then, once no request has
// answered it, as the warning that names the missing certificate. The probed
// tasks never hear about it: a tunnel says nothing about this machine's stores.
test("a connection event settles the last task, and warns once no request answers it", async () => {
  onMacHost();
  const { api, fireConnection } = mockApi(() => []); // no flows ever
  render(<App api={api} initialState={ONBOARDING} />);
  await screen.findByTestId("task-3");

  fireConnection(); // a tunnel reached the proxy; its handshake failed → no flow
  await waitFor(() =>
    expect(screen.getByTestId("task-3")).toHaveTextContent("a connection just arrived"),
  );
  expect(screen.getByTestId("task-2")).toHaveTextContent(
    "we'll confirm this on this Mac's first request",
  );

  // The screen's clock ticks on; once the grace is spent the tunnel is a failure.
  const realNow = Date.now;
  vi.spyOn(Date, "now").mockImplementation(() => realNow() + TUNNEL_GRACE_MS + 1_000);
  await waitFor(
    () => expect(screen.getByTestId("task-3")).toHaveTextContent("arriving but not decrypting"),
    { timeout: 3_000 },
  );
  // The refusal is the certificate task's to say, once its grace is spent.
  await waitFor(
    () =>
      expect(within(screen.getByTestId("task-1")).getByTestId("traffic-remedy")).toHaveTextContent(
        "does not trust Trawler's certificate yet",
      ),
    { timeout: TUNNEL_GRACE_MS + 2_000 },
  );
  // Traffic alone never reveals the list — that's still capture-only.
  expect(screen.queryByTestId("flow-row")).toBeNull();
});

// F17 / DEC-13: replacing a proxy the user set is the one setup action that
// takes something away, so it costs a confirm — and the sentence above the
// button names which proxy, from the address the same probe read.
test("replacing another proxy costs a confirm, and a refusal writes nothing", async () => {
  onMacHost();
  window.__TRAWLER_PROXY__ = "127.0.0.1:7999";
  vi.mocked(invoke).mockImplementation(async (name: string) => {
    if (name === "proxy_status") return { status: "foreign", address: "10.1.1.1:8080" };
    if (name === "confirm_replace_proxy") return false;
    return undefined;
  });
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={ONBOARDING} />);

  await waitFor(() =>
    expect(screen.getByTestId("foreign-proxy")).toHaveTextContent(
      "another proxy is set on this Mac: 10.1.1.1:8080",
    ),
  );
  fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("confirm_replace_proxy", { address: "10.1.1.1:8080" }),
  );
  expect(vi.mocked(invoke).mock.calls.filter(([n]) => n === "set_up_this_machine")).toHaveLength(0);
});

test("an allowed replacement goes through, once", async () => {
  onMacHost();
  window.__TRAWLER_PROXY__ = "127.0.0.1:7999";
  vi.mocked(invoke).mockImplementation(async (name: string) => {
    if (name === "proxy_status") return { status: "foreign", address: "10.1.1.1:8080" };
    if (name === "confirm_replace_proxy") return true;
    return undefined;
  });
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={ONBOARDING} />);

  fireEvent.click(await screen.findByRole("button", { name: "Set up this Mac" }));

  await waitFor(() =>
    expect(vi.mocked(invoke).mock.calls.filter(([n]) => n === "set_up_this_machine")).toHaveLength(1),
  );
});

// Nothing is being taken away when nothing is set, so nothing is asked.
test("setting the proxy where none is set asks no second question", async () => {
  onMacHost();
  window.__TRAWLER_PROXY__ = "127.0.0.1:7999";
  vi.mocked(invoke).mockImplementation(async (name: string) =>
    name === "proxy_status" ? { status: "none", address: null } : undefined,
  );
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={ONBOARDING} />);

  fireEvent.click(await screen.findByRole("button", { name: "Set up this Mac" }));

  await waitFor(() =>
    expect(vi.mocked(invoke).mock.calls.filter(([n]) => n === "set_up_this_machine")).toHaveLength(1),
  );
  expect(vi.mocked(invoke).mock.calls.filter(([n]) => n === "confirm_replace_proxy")).toHaveLength(0);
});

// F22: the moment setup succeeds was silent — the screen was replaced by a
// list of one row, and the outcome the design wrote was only ever seen by
// reopening setup. The seam is the transition itself: the machine's captured
// event and the list's first paint are two sides of it.
// Setup succeeding is a live decrypted request arriving while setup is open —
// the same transition the machine reads as `captured`.
async function setupSucceeds() {
  onMacHost();
  let rows: FlowSummary[] = [];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} initialState={ONBOARDING} />);
  await screen.findByText("which device's traffic do you want to see");

  rows = [flow({ uri: "https://example.com/" })];
  fire();
  return api;
}

test("the list's first paint after setup says setup worked, and offers the next device", async () => {
  await setupSucceeds();

  const banner = await screen.findByTestId("set-up-banner");
  expect(banner).toHaveTextContent("You're set up — Trawler is capturing traffic");
  expect(await screen.findByTestId("flow-row")).toBeInTheDocument();

  // It claims nothing about which device sent the request, because nothing can.
  expect(banner).not.toHaveTextContent(/from /);
});

test("the banner dismisses, and the list keeps its rows", async () => {
  await setupSucceeds();
  const banner = await screen.findByTestId("set-up-banner");

  fireEvent.click(within(banner).getByRole("button", { name: "dismiss" }));

  await waitFor(() => expect(screen.queryByTestId("set-up-banner")).toBeNull());
  expect(screen.getByTestId("flow-row")).toBeInTheDocument();
});

test("set up another device takes the user back to setup, and the banner goes with them", async () => {
  await setupSucceeds();
  const banner = await screen.findByTestId("set-up-banner");
  fireEvent.click(within(banner).getByRole("button", { name: "set up another device" }));

  expect(await screen.findByText("which device's traffic do you want to see")).toBeInTheDocument();
  expect(screen.queryByTestId("set-up-banner")).toBeNull();
});

// Review round 2, important: re-entering setup and leaving it again is the
// "already working, not news" case, and the banner announced it a second time
// as though a fresh request had just proven something. Written before the fix.
test("re-entering setup retires the banner — leaving again is not a second success", async () => {
  await setupSucceeds();
  await screen.findByTestId("set-up-banner");

  fireEvent.click(screen.getByTestId("status-dot"));
  await screen.findByText("which device's traffic do you want to see");

  fireEvent.click(screen.getByRole("button", { name: /back to flows/ }));

  await screen.findByTestId("flow-row");
  expect(screen.queryByTestId("set-up-banner")).toBeNull();
});

// A launch that finds capture already working is not news, and a core that
// stopped answering outranks good news that has already been delivered once.
test("a launch straight into the list says nothing — the moment it announces is a transition", async () => {
  onMacHost();
  const { api } = mockApi(() => [flow({ uri: "https://example.com/" })]);
  render(<App api={api} />);

  await screen.findByTestId("flow-row");
  expect(screen.queryByTestId("set-up-banner")).toBeNull();
});

// The probes are a snapshot of this machine. Two things re-take it without an
// action: the control above the steps, and the window coming back into focus —
// which is when the user has just been in System Settings.
test("check again and window focus each re-read both probes, and the reading is stamped", async () => {
  onMacHost();
  const { api } = mockApi(() => []);
  render(<App api={api} initialState={ONBOARDING} />);
  await waitFor(() => expect(screen.getByTestId("probe")).toHaveTextContent(/checked/));
  const probes = () => vi.mocked(invoke).mock.calls.filter(([name]) => name === "ca_trusted" || name === "proxy_status").length;
  expect(probes()).toBe(2);

  fireEvent.click(screen.getByRole("button", { name: "check again" }));
  await waitFor(() => expect(probes()).toBe(4));

  fireEvent(window, new Event("focus"));
  await waitFor(() => expect(probes()).toBe(6));
});

// --- the reach seam (E-ONBOARD.W3) -------------------------------------------
// The WU's contract: the screen renders the addresses the core reports, and it
// only ever asks the core to expose this machine after the shell has asked the
// user. Neither half is observable from a test of the screen alone.

const INTERFACES = [
  { name: "Wi-Fi", address: "192.168.1.24" },
  { name: "Ethernet", address: "10.0.4.7" },
];

const ARMED = {
  armed: true,
  proxy: "192.168.1.24:7420",
  api: "192.168.1.24:7421",
  ca_cert_url: "http://192.168.1.24:7421/ca.crt",
  interfaces: INTERFACES,
};

const UNARMED = { ...UNREACHABLE, interfaces: INTERFACES };

async function offMachine(over: Partial<TrawlerApi> = {}) {
  onMacHost();
  const { api } = mockApi(() => []);
  Object.assign(api, over);
  render(<App api={api} initialState={ONBOARDING} />);
  await screen.findByText(QUESTION);
  pickPlatform("iPhone or iPad");
  return api;
}

test("entering setup reads the reach state rather than assuming one", async () => {
  const reach = vi.fn(async () => UNARMED);
  const armReach = vi.fn(async () => ARMED);
  await offMachine({ reach, armReach });

  await waitFor(() => expect(reach).toHaveBeenCalledOnce());
  expect(armReach).not.toHaveBeenCalled();
  await waitFor(() =>
    expect(screen.getByTestId("task-1")).toHaveTextContent(
      "not done — Trawler answers on this machine only",
    ),
  );
});

// DEC-55: arming is an authorization act, so the ask comes first and the bind
// only on a yes. A cancelled dialog must cost nothing at all.
test("a refused confirm arms nothing — no POST, no address, no listening", async () => {
  vi.mocked(invoke).mockImplementation(async (name: string) =>
    name === "confirm_reach" ? false : undefined,
  );
  const armReach = vi.fn(async () => ARMED);
  await offMachine({ reach: async () => UNARMED, armReach });

  fireEvent.click(await screen.findByRole("button", { name: "Allow devices on my Wi-Fi to connect" }));

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("confirm_reach", { address: "192.168.1.24" }),
  );
  expect(armReach).not.toHaveBeenCalled();
  expect(screen.getByTestId("task-1")).toHaveTextContent(
    "not done — Trawler answers on this machine only",
  );
  expect(screen.queryByTestId("device-address")).toBeNull();
  // F13: the refusal reaches the screen as a refusal. Without it the dialog
  // closes, the button comes back, and nothing distinguishes a no from a yes.
  expect(await screen.findByTestId("arm-reach-status")).toHaveTextContent(
    "not allowed — Trawler still answers on this machine only",
  );
});

test("an allowed confirm arms that address, and the screen renders what answered", async () => {
  vi.mocked(invoke).mockImplementation(async (name: string) =>
    name === "confirm_reach" ? true : undefined,
  );
  const armReach = vi.fn(async () => ARMED);
  await offMachine({ reach: async () => UNARMED, armReach });

  fireEvent.click(await screen.findByRole("button", { name: "Allow devices on my Wi-Fi to connect" }));

  await waitFor(() => expect(armReach).toHaveBeenCalledWith("192.168.1.24"));
  expect(screen.queryByTestId("arm-reach-status")).toBeNull();
  const address = await screen.findByTestId("device-address");
  expect(within(address).getByRole("button", { name: /the proxy host/ })).toHaveTextContent(
    "192.168.1.24",
  );
  expect(within(address).getByRole("button", { name: /the proxy port/ })).toHaveTextContent("7420");
});

// The reversal of an exposure is not itself an exposure, so it needs no second
// dialog: nothing new is being granted by taking one away.
test("stop listening takes the exposure away without asking again", async () => {
  const disarmReach = vi.fn(async () => UNARMED);
  await offMachine({ reach: async () => ARMED, disarmReach });

  fireEvent.click(await screen.findByRole("button", { name: "stop listening" }));

  await waitFor(() => expect(disarmReach).toHaveBeenCalledOnce());
  expect(vi.mocked(invoke).mock.calls.filter(([n]) => n === "confirm_reach")).toHaveLength(0);
  await waitFor(() =>
    expect(screen.getByTestId("task-1")).toHaveTextContent(
      "not done — Trawler answers on this machine only",
    ),
  );
});

// F20: choosing "this machine" hid the exposure entirely, and the flow list -
// where a user spends the rest of the session - never showed it at all.
test("an armed reach is stated in the header, in setup and in the list alike", async () => {
  for (const phase of [ONBOARDING, PROVEN]) {
    onMacHost();
    const { api } = mockApi(() => []);
    api.reach = vi.fn(async () => ARMED);
    const disarmReach = vi.fn(async () => UNARMED);
    api.disarmReach = disarmReach;
    const view = render(<App api={api} initialState={phase} />);

    const chip = await screen.findByTestId("reach-chip");
    expect(chip).toHaveTextContent("reachable on 192.168.1.24");

    fireEvent.click(within(chip).getByRole("button", { name: "stop" }));
    await waitFor(() => expect(disarmReach).toHaveBeenCalledOnce());
    await waitFor(() => expect(screen.queryByTestId("reach-chip")).toBeNull());
    view.unmount();
  }
});

// Review round 1, blocker: the chip's control is what ends this machine's
// exposure to the network, and a rejected DELETE left the click with no trace
// at all — no disabled button, no sentence, an unhandled rejection in a console
// nobody reads. Button's contract asks for both halves, which is what the
// identical action gets one file away.
test("a stop that fails says so, and leaves the exposure it could not end on the screen", async () => {
  onMacHost();
  const { api } = mockApi(() => []);
  api.reach = vi.fn(async () => ARMED);
  api.disarmReach = vi.fn(async () => {
    throw new Error("core said no");
  });
  render(<App api={api} initialState={PROVEN} />);

  const chip = await screen.findByTestId("reach-chip");
  fireEvent.click(within(chip).getByRole("button", { name: "stop" }));

  await waitFor(() =>
    expect(screen.getByTestId("reach-chip")).toHaveTextContent("couldn't stop listening"),
  );
  expect(screen.getByTestId("reach-chip")).toHaveTextContent("reachable on 192.168.1.24");
});

test("the stop control is out of reach while its call is in flight, and says why", async () => {
  onMacHost();
  const { api } = mockApi(() => []);
  api.reach = vi.fn(async () => ARMED);
  let release: (state: typeof UNARMED) => void = () => {};
  api.disarmReach = vi.fn(() => new Promise<typeof UNARMED>((resolve) => (release = resolve)));
  render(<App api={api} initialState={PROVEN} />);

  const chip = await screen.findByTestId("reach-chip");
  fireEvent.click(within(chip).getByRole("button", { name: "stop" }));

  await waitFor(() => expect(screen.getByRole("button", { name: "stop" })).toBeDisabled());
  expect(screen.getByTestId("reach-chip")).toHaveTextContent("stopping…");

  release(UNARMED);
  await waitFor(() => expect(screen.queryByTestId("reach-chip")).toBeNull());
});

// The core answers with kernel device names because it may not ask the host
// what its owner calls them; the shell asks, and the screen reads the answer.
test("an address is offered under the name its owner sees, when the host can say", async () => {
  onMacHost();
  vi.mocked(invoke).mockImplementation(async (name: string) =>
    name === "interface_names" ? { en1: "Wi-Fi" } : undefined,
  );
  const { api } = mockApi(() => []);
  api.reach = vi.fn(async () => ({
    ...UNARMED,
    interfaces: [
      { name: "en1", address: "192.168.1.24" },
      { name: "en5", address: "10.0.4.7" },
    ],
  }));
  render(<App api={api} initialState={ONBOARDING} />);
  await screen.findByText(QUESTION);
  pickPlatform("iPhone or iPad");

  await waitFor(() => expect(screen.getByTestId("listen-on")).toHaveTextContent("Wi-Fi · 192.168.1.24"));
  fireEvent.click(screen.getByText("other addresses"));
  // A device the host named nothing keeps the name the kernel gave it.
  expect(screen.getByRole("button", { name: "en5 · 10.0.4.7" })).toBeInTheDocument();
});

test("a shell that cannot name a device leaves the name the core reported", async () => {
  onMacHost();
  vi.mocked(invoke).mockImplementation(async (name: string) => {
    if (name === "interface_names") throw new Error("no shell here");
    return undefined;
  });
  const { api } = mockApi(() => []);
  api.reach = vi.fn(async () => ({
    ...UNARMED,
    interfaces: [
      { name: "en1", address: "192.168.1.24" },
      { name: "en5", address: "10.0.4.7" },
    ],
  }));
  render(<App api={api} initialState={ONBOARDING} />);
  await screen.findByText(QUESTION);
  pickPlatform("iPhone or iPad");

  await waitFor(() => expect(screen.getByTestId("listen-on")).toHaveTextContent("en1 · 192.168.1.24"));
});

test("a machine nothing is listening on carries no chip at all", async () => {
  onMacHost();
  const { api } = mockApi(() => []);
  api.reach = vi.fn(async () => UNARMED);
  render(<App api={api} initialState={PROVEN} />);

  await waitFor(() => expect(api.reach).toHaveBeenCalledOnce());
  expect(screen.queryByTestId("reach-chip")).toBeNull();
});

test("a core that will not say leaves the screen saying Trawler answers here only", async () => {
  await offMachine({
    reach: async () => {
      throw new Error("no core");
    },
  });

  await waitFor(() =>
    expect(screen.getByTestId("task-1")).toHaveTextContent(
      "not done — Trawler answers on this machine only",
    ),
  );
  expect(screen.queryByTestId("device-address")).toBeNull();
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

// The launch gate (E-ONBREV.W2). The launch read decides whether setup is owed,
// so until it answers nothing setup would say is drawn, and a read nobody
// answered decides nothing. The stream's handlers are held so a test can drop
// it and bring it back, the two things a core does between keystrokes.
function streamApi(answer: () => Promise<FlowSummary[]>) {
  const handlers: { current: StreamHandlers | null } = { current: null };
  const api = apiStub({
    listFlows: () => answer(),
    stream: (h: StreamHandlers) => {
      handlers.current = h;
      return () => {};
    },
  });
  return { api, down: () => act(() => handlers.current?.onDown?.()), up: () => act(() => handlers.current?.onUp?.()) };
}

const FAILED = () => Promise.reject(new TypeError("Failed to fetch"));
const NEVER = () => new Promise<FlowSummary[]>(() => {});

// R1: asserted on the synchronous render, the paint a findBy… query waits past.
test("while the launch read is in flight only the shell and the checking dot paint", async () => {
  const { api } = shellApi(NEVER);
  render(<App api={api} />);
  expect(screen.getByTestId("status-dot")).toHaveAttribute("data-status", "checking");
  expect(screen.queryByText(QUESTION)).toBeNull();
  expect(screen.queryByTestId("task-1")).toBeNull();

  await act(() => new Promise((settle) => setTimeout(settle, 200)));
  expect(screen.queryByText(QUESTION)).toBeNull();
  expect(screen.queryByText("trawler is not answering")).toBeNull();
});

// R2: a core that is not running is not answered with setup steps, and the
// captured rows it holds when it comes back are a success from before.
test("a launch the core does not answer says so, and its retry reads again", async () => {
  let answer: () => Promise<FlowSummary[]> = FAILED;
  const { api } = shellApi(() => answer());
  render(<App api={api} />);

  const region = await screen.findByTestId("empty");
  expect(region).toHaveTextContent("trawler is not answering");
  expect(region).toHaveTextContent(`no answer from ${apiAddr()}`);
  expect(screen.queryByText(QUESTION)).toBeNull();
  expect(screen.getByTestId("status-dot")).toHaveAttribute("data-status", "checking");

  answer = async () => [HTTPS];
  fireEvent.click(screen.getByRole("button", { name: "retry" }));

  expect(await screen.findByTestId("flow-row")).toBeInTheDocument();
  expect(screen.queryByTestId("set-up-banner")).toBeNull();
});

test("the stream coming back ends a silent launch, and an empty core then opens setup", async () => {
  let answer: () => Promise<FlowSummary[]> = FAILED;
  const { api, up } = streamApi(() => answer());
  render(<App api={api} />);
  await screen.findByText("trawler is not answering");

  answer = async () => [];
  up();

  expect(await screen.findByText(QUESTION)).toBeInTheDocument();
  expect(screen.queryByText("trawler is not answering")).toBeNull();
});

test("a stream that drops while the launch read hangs is the same silence", async () => {
  const { api, down } = streamApi(NEVER);
  render(<App api={api} />);
  down();

  expect(await screen.findByText("trawler is not answering")).toBeInTheDocument();
  expect(screen.queryByText(QUESTION)).toBeNull();
});

test("the setup screen states a core that stops answering above its steps", async () => {
  const { api, down } = streamApi(async () => []);
  render(<App api={api} />);
  await screen.findByText(QUESTION);
  expect(screen.queryByTestId("band")).toBeNull();

  down();

  const band = await screen.findByTestId("band");
  expect(band).toHaveTextContent("trawler is not answering");
  expect(band).toHaveTextContent(`no answer from ${apiAddr()}`);
  expect(screen.getByText(QUESTION)).toBeInTheDocument();
});

test("a captured row that started before setup opened reveals the list, and announces nothing", async () => {
  let rows: FlowSummary[] = [];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} />);
  await screen.findByText(QUESTION);

  rows = [flow({ uri: "https://example.com/", started_at: new Date(Date.now() - 60_000).toISOString() })];
  fire();

  expect(await screen.findByTestId("flow-row")).toBeInTheDocument();
  expect(screen.queryByTestId("set-up-banner")).toBeNull();
});

test("a captured row that started after setup opened is the success the banner announces", async () => {
  let rows: FlowSummary[] = [];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} />);
  await screen.findByText(QUESTION);

  rows = [flow({ uri: "https://example.com/", started_at: new Date().toISOString() })];
  fire();

  expect(await screen.findByTestId("set-up-banner")).toBeInTheDocument();
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

// E-REPLAY.W3 — the composer against the send doors, through the scripted client.
// The stub answers the way the core does: a send lands a sent flow in the store
// and answers it; the list only moves when the bus says so.
function replayWorld() {
  const b64 = (text: string) => btoa(text);
  const views: Record<string, FlowView> = {
    f1: {
      ...flow({ id: "f1", method: "POST", uri: "https://a.test/users", status: 201 }),
      request_http_version: "HTTP/1.1",
      response_http_version: "HTTP/1.1",
      request_headers: [
        { name: "Host", value: "a.test" },
        { name: "Authorization", value: "Bearer OLD" },
        { name: "Content-Length", value: "12" },
      ],
      response_headers: [],
      request_body: { state: "captured", size: 12, data_base64: b64('{"name":"a"}') },
      response_body: { state: "empty" },
    },
    f2: {
      ...flow({ id: "f2", uri: "https://b.test/", started_at: "2026-07-12T10:00:01Z" }),
      request_http_version: "HTTP/1.1",
      response_http_version: "HTTP/1.1",
      request_headers: [{ name: "Authorization", value: "Bearer TWO" }],
      response_headers: [],
      request_body: { state: "empty" },
      response_body: { state: "empty" },
    },
  };
  let rows: FlowSummary[] = [views.f1, views.f2].map(({ id, started_at, duration_ms, method, uri, status, source, seed }) =>
    flow({ id, started_at, duration_ms, method, uri, status, source, seed }),
  );
  const world = mockApi(() => rows);
  const land = (seed: string | null, method: string, uri: string): FlowView => {
    const id = `sent-${rows.length}`;
    const view: FlowView = { ...views[seed ?? "f2"], id, seed, source: "sent", method, uri, status: 200, duration_ms: 142 };
    views[id] = view;
    rows = [...rows, flow({ id, method, uri, status: 200, source: "sent", seed, started_at: "2026-07-12T10:00:02Z" })];
    return view;
  };
  world.api.getFlow = vi.fn(async (id: string) => views[id]);
  world.api.replay = vi.fn(async (id: string, fields) => land(id, fields.method ?? views[id].method, fields.url ?? views[id].uri));
  world.api.send = vi.fn(async (fields) => land(null, fields.method ?? "GET", fields.url ?? ""));
  return world;
}

const rowOf = async (index: number) => (await screen.findAllByTestId("flow-row"))[index];

async function replayFirst() {
  fireEvent.click(await rowOf(0));
  fireEvent.click(await screen.findByRole("button", { name: "replay" }));
  return screen.findByTestId("composer");
}

test("replay with one edit sends exactly that edit, and the result lands as a sent flow", async () => {
  const { api, fire } = replayWorld();
  render(<App api={api} initialState={PROVEN} />);

  await replayFirst();
  expect(screen.getByRole("textbox", { name: "url" })).toHaveFocus();
  fireEvent.change(screen.getByRole("textbox", { name: "Authorization value" }), { target: { value: "Bearer NEW" } });
  fireEvent.click(screen.getByRole("button", { name: "send" }));

  await within(screen.getByTestId("send-zone")).findByRole("button", { name: "open the new flow" });
  // The Host row goes by omission and the core-owned Content-Length never
  // entered the draft: the list sent is exactly what the core should send.
  expect(api.replay).toHaveBeenCalledWith("f1", { headers: [{ name: "Authorization", value: "Bearer NEW" }] });
  expect(screen.getByTestId("send-zone")).toHaveTextContent("142 ms");

  await fire();
  await waitFor(() => expect(screen.getAllByTestId("flow-row")).toHaveLength(3));
  expect(within(screen.getAllByTestId("flow-row")[2]).getByLabelText("sent")).toHaveTextContent("↗");

  fireEvent.click(screen.getByRole("button", { name: "open the new flow" }));
  await waitFor(() => expect(api.getFlow).toHaveBeenLastCalledWith("sent-2"));
  expect(await screen.findByTestId("detail")).toBeInTheDocument();
});

test("the draft survives looking away, the door that made it reopens it, and another flow replaces it", async () => {
  const { api } = replayWorld();
  render(<App api={api} initialState={PROVEN} />);

  await replayFirst();
  fireEvent.change(screen.getByRole("textbox", { name: "Authorization value" }), { target: { value: "Bearer NEW" } });

  fireEvent.click(await rowOf(1));
  await screen.findByTestId("detail");
  expect(screen.queryByTestId("composer")).toBeNull();

  await replayFirst();
  expect(screen.getByRole("textbox", { name: "Authorization value" })).toHaveValue("Bearer NEW");

  fireEvent.click(await rowOf(1));
  fireEvent.click(await screen.findByRole("button", { name: "replay" }));
  await screen.findByTestId("composer");
  expect(screen.getByRole("textbox", { name: "url" })).toHaveValue("https://b.test/");
  expect(screen.getByRole("textbox", { name: "Authorization value" })).toHaveValue("Bearer TWO");
});

test("× on a seeded draft returns to the seed's inspector, and the draft is kept", async () => {
  const { api } = replayWorld();
  render(<App api={api} initialState={PROVEN} />);

  await replayFirst();
  fireEvent.change(screen.getByRole("textbox", { name: "url" }), { target: { value: "https://c.test/" } });
  fireEvent.click(screen.getByRole("button", { name: "back to the flow" }));

  const detail = await screen.findByTestId("detail");
  expect(detail).toHaveTextContent("https://a.test/users");
  fireEvent.click(within(detail).getByRole("button", { name: "replay" }));
  expect(await screen.findByRole("textbox", { name: "url" })).toHaveValue("https://c.test/");
});

test("a request from scratch reaches the compose door, and the draft stays editable after its send", async () => {
  const { api } = replayWorld();
  render(<App api={api} initialState={PROVEN} />);
  await rowOf(0);

  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.trigger }));
  fireEvent.click(screen.getByRole("menuitem", { name: "new request" }));
  await screen.findByTestId("composer");
  const send = screen.getByRole("button", { name: "send" });
  expect(send).toBeDisabled();
  expect(screen.getByTestId("send-zone")).toHaveTextContent("a request needs a URL");

  fireEvent.change(screen.getByRole("textbox", { name: "url" }), { target: { value: "https://c.test/ping" } });
  fireEvent.click(send);

  await within(screen.getByTestId("send-zone")).findByRole("button", { name: "open the new flow" });
  expect(api.send).toHaveBeenCalledWith({ method: "GET", url: "https://c.test/ping" });
  expect(api.replay).not.toHaveBeenCalled();
  const url = screen.getByRole("textbox", { name: "url" });
  expect(url).toBeEnabled();
  fireEvent.change(url, { target: { value: "https://c.test/pong" } });
  expect(url).toHaveValue("https://c.test/pong");

  fireEvent.click(screen.getByRole("button", { name: "close" }));
  await waitFor(() => expect(screen.queryByTestId("composer")).toBeNull());
  expect(screen.queryByTestId("detail")).toBeNull();
});

test.each([
  ["a refusal the core would not attempt", "the seed's request body was not captured (too_large); give a body to replace it"],
  ["a send that got no reply", "send failed: client error (Connect): tcp connect error: Connection refused (os error 61)"],
])("%s is shown verbatim beside send, and the list does not move", async (_, sentence) => {
  const { api, fire } = replayWorld();
  api.replay = vi.fn(async () => {
    throw new Error(sentence);
  });
  render(<App api={api} initialState={PROVEN} />);

  await replayFirst();
  fireEvent.click(screen.getByRole("button", { name: "send" }));

  const failed = await screen.findByTestId("send-failed");
  expect(failed).toHaveTextContent(`⚠${sentence}`);
  expect(within(screen.getByTestId("send-zone")).queryByLabelText(/^\d{3}\b/)).toBeNull();
  expect(screen.getByRole("button", { name: "send" })).toBeEnabled();
  await fire();
  expect(screen.getAllByTestId("flow-row")).toHaveLength(2);
});

test("a reply that lands after its draft was replaced never speaks for the new one", async () => {
  const { api } = replayWorld();
  let answer: (view: FlowView) => void = () => {};
  api.replay = vi.fn(() => new Promise<FlowView>((resolve) => (answer = resolve)));
  render(<App api={api} initialState={PROVEN} />);

  await replayFirst();
  fireEvent.click(screen.getByRole("button", { name: "send" }));
  expect(screen.getByTestId("send-zone")).toHaveTextContent("sending…");

  fireEvent.click(await rowOf(1));
  fireEvent.click(await screen.findByRole("button", { name: "replay" }));
  await screen.findByTestId("composer");
  await act(async () => answer({ ...(await api.getFlow("f1")), id: "late", status: 500 }));

  expect(screen.getByTestId("send-zone")).toHaveTextContent("not sent yet");
  expect(screen.getByRole("button", { name: "send" })).toBeEnabled();
});

test("new request over an open replay draft opens a fresh composer, with focus in its URL field", async () => {
  const { api } = replayWorld();
  render(<App api={api} initialState={PROVEN} />);
  await replayFirst();
  expect(screen.getByRole("textbox", { name: "url" })).toHaveValue("https://a.test/users");

  fireEvent.click(screen.getByRole("button", { name: CAPTURE_COPY.trigger }));
  fireEvent.click(screen.getByRole("menuitem", { name: "new request" }));

  const url = await screen.findByRole("textbox", { name: "url" });
  await waitFor(() => expect(url).toHaveValue(""));
  expect(url).toHaveFocus();
});
