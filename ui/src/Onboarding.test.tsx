import { afterEach, beforeEach, describe, expect, it, test, vi } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { Onboarding } from "./Onboarding";
import { COPY, type OnbState } from "./onboardingMachine";
import * as setup from "./setup";
import { NO_TRAFFIC, RECENT_MS, type Traffic } from "./verdicts";
import type { ReachState } from "./api";

vi.mock("./setup", async (importOriginal) => {
  const mod = await importOriginal<typeof import("./setup")>();
  return { ...mod, setupFor: vi.fn(mod.setupFor) };
});

const onboarding: OnbState = { phase: "onboarding", step: "trust-ca", everProven: false };
const ADDR = "127.0.0.1:9999";

const INTERFACES = [
  { name: "Wi-Fi", address: "192.168.1.24" },
  { name: "Ethernet", address: "10.0.4.7" },
];

const UNARMED: ReachState = {
  armed: false,
  proxy: null,
  api: null,
  ca_cert_url: null,
  interfaces: INTERFACES,
};

const ARMED: ReachState = {
  armed: true,
  proxy: "192.168.1.24:9999",
  api: "192.168.1.24:7421",
  ca_cert_url: "http://192.168.1.24:7421/ca.crt",
  interfaces: INTERFACES,
};

const ago = (ms: number): number => Date.now() - ms;
const capturing: Traffic = { ...NO_TRAFFIC, lastDecryptedAt: ago(1_000) };
const arriving: Traffic = { ...NO_TRAFFIC, lastTunnelAt: ago(5_000) };

const draw = (props: Partial<Parameters<typeof Onboarding>[0]> = {}) =>
  render(
    <Onboarding
      state={onboarding}
      dispatch={vi.fn()}
      os="macos"
      proxyAddr={ADDR}
      onRecheck={vi.fn()}
      onInstallCa={vi.fn(async () => {})}
      onSaveCa={vi.fn(async () => null)}
      onSetProxy={vi.fn(async () => {})}
      onUnsetProxy={vi.fn(async () => {})}
      onArmReach={vi.fn(async () => {})}
      onDisarmReach={vi.fn(async () => {})}
      {...props}
    />,
  );

// The screen's one axis. Choosing is what every zone below reads, so the tests
// choose the way a user does rather than by prop.
const choose = (label: string) => {
  fireEvent.click(screen.getByRole("button", { name: "platform" }));
  fireEvent.click(screen.getByRole("option", { name: label }));
};

const zone = (name: string) => screen.queryByTestId(name);
const disclosure = (precondition: string) =>
  screen.getByTestId(`precondition-${precondition}`).querySelector("details");

afterEach(() => vi.clearAllMocks());

describe("the screen's one axis", () => {
  it("offers the eight platforms in one Select and asks the question once", () => {
    draw();
    fireEvent.click(screen.getByRole("button", { name: "platform" }));
    expect(screen.getAllByRole("option")).toHaveLength(8);
    expect(screen.getByText("what are you setting up")).toBeInTheDocument();
  });

  it("has no second target picker anywhere below it", () => {
    draw();
    expect(screen.queryByRole("button", { name: /trust target|proxy target/ })).toBeNull();
    expect(screen.queryByText(/steps for/)).toBeNull();
  });

  it("selects this machine by default, named for the detected host", () => {
    draw();
    expect(screen.getByRole("button", { name: "platform" })).toHaveTextContent(
      "This machine — macOS",
    );
  });

  // A stepper needs per-step progress, which nothing observes; the axis is a
  // selection, never a position in a sequence.
  it("draws no stepper: no numbering, and no control that moves a cursor", () => {
    const dispatch = vi.fn();
    const { container } = draw({ dispatch });
    expect(container.textContent).not.toMatch(/\b1 · |\b2 · |\b3 · /);
    expect(container.querySelector("[data-testid^='step-']")).toBeNull();
    for (const button of screen.getAllByRole("button")) fireEvent.click(button);
    expect(dispatch).not.toHaveBeenCalledWith(expect.objectContaining({ t: "goto" }));
  });
});

describe("the probe row answers only where a probe can", () => {
  it("is there for this machine", () => {
    draw({ probedAt: ago(60_000) });
    expect(zone("probe")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "check again" })).toBeInTheDocument();
  });

  it("is there for the Simulator, whose routing this machine's probe does read", () => {
    draw({ probedAt: ago(60_000) });
    choose("iOS Simulator");
    expect(zone("probe")).toBeInTheDocument();
  });

  it.each(["iPhone or iPad", "Another Windows PC", "Android emulator"])(
    "is absent under %s, because there is nothing here it could read",
    (platform) => {
      draw({ probedAt: ago(60_000) });
      choose(platform);
      expect(zone("probe")).toBeNull();
    },
  );

  it("says what it is doing while it reads, and dates the reading after", () => {
    const { rerender } = draw({ probing: true });
    expect(zone("probe")).toHaveTextContent("checking this machine…");
    expect(screen.getByRole("button", { name: "check again" })).toBeDisabled();

    rerender(
      <Onboarding state={onboarding} dispatch={vi.fn()} probing={false} probedAt={ago(60_000)} onRecheck={vi.fn()} />,
    );
    expect(zone("probe")).toHaveTextContent(/checked/);
  });
});

describe("the un-probeable verdicts carry their own words", () => {
  it("never borrows this machine's trust answer for another device", () => {
    draw({ trustProbe: "trusted" });
    choose("iPhone or iPad");
    const trust = screen.getByTestId("precondition-trust");
    expect(trust).toHaveTextContent("can't check another device from here");
    expect(trust).not.toHaveTextContent("trusted on this machine");
  });

  it("renders no routing verdict off-machine rather than this machine's", () => {
    draw({ proxyProbe: "ours" });
    choose("Android device");
    expect(screen.getByTestId("precondition-route")).not.toHaveTextContent(
      "system proxy is set to Trawler",
    );
  });

  it("says can't-check on this platform where the probe itself came back unknown", () => {
    draw({ trustProbe: "unknown" });
    expect(screen.getByTestId("precondition-trust")).toHaveTextContent("can't check on this platform");
  });
});

describe("the one-click actions belong to this machine alone", () => {
  it("offers install and set-proxy here", () => {
    draw();
    expect(screen.getByRole("button", { name: "install certificate" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "set system proxy" })).toBeInTheDocument();
  });

  it.each(["iPhone or iPad", "Another Mac", "Android emulator"])(
    "offers neither under %s",
    (platform) => {
      draw();
      choose(platform);
      expect(screen.queryByRole("button", { name: "install certificate" })).toBeNull();
      expect(screen.queryByRole("button", { name: "set system proxy" })).toBeNull();
    },
  );

  it("offers neither on a host with no one-click path", () => {
    draw({ os: "linux" });
    expect(screen.queryByRole("button", { name: "install certificate" })).toBeNull();
    expect(screen.queryByRole("button", { name: "set system proxy" })).toBeNull();
  });

  // DEC-13's inherited assertion: never offer to undo something we did not do.
  it("offers stop routing only once a proxy here is ours", () => {
    draw({ proxyProbe: "none" });
    expect(screen.queryByRole("button", { name: "stop routing through Trawler" })).toBeNull();

    draw({ proxyProbe: "ours" });
    expect(screen.getByRole("button", { name: "stop routing through Trawler" })).toBeInTheDocument();
  });

  // The other half of the same rule: an action that changed this host stays
  // reversible even where the probe that would confirm it cannot answer.
  it("offers stop routing after setting the proxy, whatever the probe can say", async () => {
    draw({ proxyProbe: "unknown", onSetProxy: vi.fn(async () => {}) });
    fireEvent.click(screen.getByRole("button", { name: "set system proxy" }));
    expect(
      await screen.findByRole("button", { name: "stop routing through Trawler" }),
    ).toBeInTheDocument();
  });
});

// F15: the Simulator was probed as this machine and actionable as nothing. It
// shares one of this Mac's two stores, and the screen now answers per store.
describe("the Simulator acts through what it shares, and only through that", () => {
  beforeEach(() => {
    draw({ trustProbe: "trusted", proxyProbe: "none" });
    choose("iOS Simulator");
  });

  it("offers the proxy this Mac sets, because the Simulator routes through it", () => {
    expect(screen.getByRole("button", { name: "set system proxy" })).toBeInTheDocument();
    expect(screen.getByTestId("precondition-route")).toHaveTextContent(
      "not set on this machine yet",
    );
  });

  it("offers no certificate install, and never reads this Mac's answer as the Simulator's", () => {
    const trust = screen.getByTestId("precondition-trust");
    expect(screen.queryByRole("button", { name: "install certificate" })).toBeNull();
    expect(trust).not.toHaveTextContent("trusted on this machine");
    expect(trust).toHaveTextContent("keeps its own certificate store");
  });

  it("gives it the trust route it can follow", () => {
    expect(screen.getByTestId("precondition-trust")).toHaveTextContent(
      "xcrun simctl keychain booted add-root-cert",
    );
  });
});

describe("an off-machine platform with nothing to point at", () => {
  beforeEach(() => {
    draw({ reach: UNARMED });
    choose("iPhone or iPad");
  });

  it("states the precondition that cannot be met, in its own zone", () => {
    expect(screen.getByTestId("precondition-reach")).toHaveTextContent(
      "Trawler answers on this machine only",
    );
  });

  it("offers the action that meets it, and says the ask comes first", () => {
    expect(screen.getByRole("button", { name: "let devices reach Trawler" })).toBeInTheDocument();
    expect(screen.getByTestId("precondition-reach")).toHaveTextContent(
      "Trawler will ask before it starts listening",
    );
  });

  it("offers no step that cannot work", () => {
    expect(screen.queryByRole("group", { name: "set it up by hand" })).toBeNull();
    expect(screen.getByTestId("precondition-trust")).toHaveTextContent(
      "the certificate route opens once Trawler is reachable",
    );
    expect(screen.getByTestId("precondition-route")).toHaveTextContent(
      "there is no address to point the device at yet",
    );
  });

  // The addresses on offer are the choice; an address in an instruction would
  // be a promise that a socket is listening there, which is the lie the whole
  // reach seam exists to prevent.
  it("prints no address to point the device at — not a loopback one, not a plausible one", () => {
    for (const precondition of ["trust", "route"]) {
      const text = screen.getByTestId(`precondition-${precondition}`).textContent ?? "";
      expect(text).not.toContain("127.0.0.1");
      expect(text).not.toContain("192.168.1.24");
    }
    expect(zone("device-address")).toBeNull();
  });

  it("says there is nothing to try yet", () => {
    expect(zone("try")).toHaveTextContent("nothing to try until Trawler is reachable");
  });

  it("does not offer to stop something it never started", () => {
    expect(screen.queryByRole("button", { name: "stop listening" })).toBeNull();
  });
});

describe("an off-machine platform once reach is armed", () => {
  beforeEach(() => {
    draw({ reach: ARMED });
    choose("iPhone or iPad");
  });

  it("says where it is listening, and for how long", () => {
    expect(screen.getByTestId("precondition-reach")).toHaveTextContent(
      "listening on 192.168.1.24 — until Trawler quits",
    );
  });

  it("renders the server and the port the device's settings want, as values to take", () => {
    const address = screen.getByTestId("device-address");
    expect(within(address).getByRole("button", { name: /the proxy host/ })).toHaveTextContent(
      "192.168.1.24",
    );
    expect(within(address).getByRole("button", { name: /the proxy port/ })).toHaveTextContent("9999");
  });

  it("renders the certificate URL a device can open, without opening anything first", () => {
    expect(screen.getByTestId("precondition-trust")).toHaveTextContent(
      "http://192.168.1.24:7421/ca.crt",
    );
  });

  it("offers the reversal it now owes", () => {
    expect(screen.getByRole("button", { name: "stop listening" })).toBeInTheDocument();
  });

  it("says what to try on the device", () => {
    expect(zone("try")).toHaveTextContent("open any site on the device");
  });
});

// F11: four choices, three of them rotating IPv6 addresses, in a control that
// overflowed its column. The core now answers best-first, so the screen offers
// the first one and keeps the rest where a user who wants them can find them.
describe("the address to listen on is one offer, with the rest behind it", () => {
  const withInterfaces = (count: number) =>
    draw({
      reach: {
        ...UNARMED,
        interfaces: Array.from({ length: count }, (_, i) => ({
          name: `if${i}`,
          address: `10.0.0.${i + 1}`,
        })),
      },
    });

  it("offers no control for one address — the action names it instead", () => {
    withInterfaces(1);
    choose("iPhone or iPad");
    expect(zone("listen-on")).toBeNull();
    expect(
      screen.getByRole("button", { name: "let devices reach Trawler at 10.0.0.1" }),
    ).toBeInTheDocument();
  });

  it.each([2, 5])("offers the first of %i, named, and hides the others", (count) => {
    withInterfaces(count);
    choose("iPhone or iPad");
    expect(zone("listen-on")).toHaveTextContent("if0 · 10.0.0.1");
    const others = screen.getByText("other addresses").closest("details");
    expect(within(others!).getAllByRole("button")).toHaveLength(count - 1);
    expect(within(others!).getByRole("button", { name: "if1 · 10.0.0.2" })).toBeInTheDocument();
  });

  it("arms the address the user picked, not the one that happened to be first", () => {
    const onArmReach = vi.fn(async () => {});
    draw({ reach: UNARMED, onArmReach });
    choose("iPhone or iPad");
    fireEvent.click(screen.getByText("other addresses"));
    fireEvent.click(screen.getByRole("button", { name: "Ethernet · 10.0.4.7" }));
    expect(zone("listen-on")).toHaveTextContent("Ethernet · 10.0.4.7");
    fireEvent.click(screen.getByRole("button", { name: "let devices reach Trawler" }));
    expect(onArmReach).toHaveBeenCalledWith("10.0.4.7");
  });

  it("says so when the host holds no address to offer, and offers no action", () => {
    withInterfaces(0);
    choose("iPhone or iPad");
    expect(screen.getByTestId("precondition-reach")).toHaveTextContent(
      "this machine holds no network address",
    );
    expect(screen.queryByRole("button", { name: /let devices reach Trawler/ })).toBeNull();
  });
});

describe("the emulator is not a device, and says so", () => {
  it("needs no reach: it renders its steps with reach unarmed", () => {
    draw({ reach: UNARMED });
    choose("Android emulator");
    expect(zone("precondition-reach")).toBeNull();
    expect(screen.getByTestId("precondition-route")).toHaveTextContent("10.0.2.2");
  });
});

describe("the lead verdict, and the outcome that leaves it", () => {
  it("leads with a pending sentence while the launch check runs", () => {
    draw({ state: { ...onboarding, phase: "checking" } });
    expect(zone("lead-verdict")).toHaveTextContent("checking what is already set up");
    expect(screen.getAllByTestId("spinner").length).toBeGreaterThan(0);
  });

  // The screen may not state two contradictory readings of one fact. While the
  // launch check runs, a flow the stream already delivered must not let the try
  // zone announce capture under a headline that says it is still checking.
  it("never announces capture under a headline that is still checking", () => {
    draw({
      state: { ...onboarding, phase: "checking" },
      traffic: { ...NO_TRAFFIC, lastDecryptedAt: ago(1_000), ever: true },
    });

    expect(zone("lead-verdict")).toHaveTextContent("checking what is already set up");
    expect(zone("proven")).toBeNull();
  });

  it("separates nothing-ever from nothing-now", () => {
    draw({ traffic: NO_TRAFFIC });
    expect(zone("lead-verdict")).toHaveTextContent("nothing has reached Trawler yet");

    draw({ traffic: { ...NO_TRAFFIC, ever: true } });
    expect(screen.getAllByTestId("lead-verdict")[1]).toHaveTextContent("nothing is arriving now");
  });

  it("names the arriving failure and sends the reader to the step that fixes it", () => {
    draw({ traffic: arriving });
    expect(zone("lead-verdict")).toHaveTextContent("traffic is arriving, but nothing is decrypting");
    expect(zone("trust-remedy")).toHaveTextContent("does not trust the certificate yet");
  });

  it("renders the remedy beside the precondition it instructs, not in the headline", () => {
    draw({ traffic: arriving });
    expect(screen.getByTestId("precondition-trust")).toContainElement(screen.getByTestId("trust-remedy"));
  });

  it("puts the proven outcome under the request that made it, and clears the headline", () => {
    draw({ traffic: capturing });
    expect(zone("lead-verdict")).toBeNull();
    expect(zone("try")).toContainElement(screen.getByTestId("proven"));
    expect(zone("proven")).toHaveTextContent("Trawler is capturing");
  });
});

// UC-ONBOARD's promises about the manual route, re-checked against the tree
// rather than trusted from the run's audit: the guide is a fallback where an
// action leads, and the whole instruction where none does.
describe("the guide is hidden only where something else is offered first", () => {
  it("renders inline on a host with no one-click path", () => {
    draw({ os: "linux" });
    expect(screen.getByTestId("precondition-trust")).toHaveTextContent(/update-ca-certificates/);
    expect(within(screen.getByTestId("precondition-trust")).queryByText("set it up by hand")).toBeNull();
  });

  it("opens itself when the action it backs up fails", async () => {
    draw({ onInstallCa: vi.fn(async () => Promise.reject(new Error("declined"))) });
    fireEvent.click(screen.getByRole("button", { name: "install certificate" }));

    await screen.findByText(/the steps below do it by hand/);
    expect(disclosure("trust")).toHaveAttribute("open");
  });

  it("stays shut while the action is still the better route", () => {
    draw();
    expect(disclosure("trust")).not.toHaveAttribute("open");
  });

  // The rule itself, over every platform: a summary reading "set it up by hand"
  // claims an automatic route exists. Where none is offered, the steps are the
  // instruction and render open — which is also what numbers them.
  it.each([
    "This machine — macOS",
    "iOS Simulator",
    "Android emulator",
    "Another Mac",
    "Another Windows PC",
    "Another Ubuntu machine",
    "iPhone or iPad",
    "Android device",
  ])("under %s, each guide is behind a summary exactly where its action is offered", (label) => {
    draw({ reach: ARMED });
    choose(label);
    const installs = screen.queryByRole("button", { name: "install certificate" }) !== null;
    const routes = screen.queryByRole("button", { name: "set system proxy" }) !== null;
    expect(disclosure("trust") !== null).toBe(installs);
    expect(disclosure("route") !== null).toBe(routes);
  });

  it("gives a phone its steps as the instruction, not as a fallback", () => {
    draw({ reach: ARMED });
    choose("iPhone or iPad");
    const trust = screen.getByTestId("precondition-trust");
    expect(within(trust).queryByText("set it up by hand")).toBeNull();
    expect(trust).toHaveTextContent("VPN & Device Management");
  });
});

// An idle verdict that drops the date is the one that started DEC-33: the user
// cannot tell "set up, quiet now" from "never worked" without it.
test("idle dates the last flow rather than repeating the setup advice", () => {
  draw({ traffic: { ...NO_TRAFFIC, lastDecryptedAt: ago(RECENT_MS * 4), ever: true } });
  expect(zone("lead-verdict")).toHaveTextContent(/last flow/);
});

// StatusLine renders the verdict and refuses its instruction, so the screen owes
// the second element. UC-ONBOARD: "the verdict and what to do about it are two
// elements, so a short verdict never costs the instruction".
describe("a verdict that implies an instruction renders both", () => {
  it("points an un-probeable platform at the answer that will settle it", () => {
    draw({ trustProbe: "unknown" });
    expect(screen.getByTestId("trust-guidance")).toHaveTextContent(
      "the traffic verdict is what proves it",
    );
  });

  it("names the conflict a foreign proxy is, and what to do about it", () => {
    draw({ proxyProbe: "foreign" });
    expect(screen.getByTestId("route-guidance")).toHaveTextContent(/point it at Trawler here/);
  });

  it("states a failure once, not twice", async () => {
    draw({ onInstallCa: vi.fn(async () => Promise.reject(new Error("user declined"))) });
    fireEvent.click(screen.getByRole("button", { name: "install certificate" }));

    await screen.findByText(/the steps below do it by hand/);
    expect(screen.getByTestId("install-ca-status")).toHaveTextContent("user declined");
    expect(zone("trust-guidance")).toBeNull();
  });

  it("says nothing extra where the verdict is the whole answer", () => {
    draw({ trustProbe: "trusted", proxyProbe: "ours" });
    expect(zone("trust-guidance")).toBeNull();
    expect(zone("route-guidance")).toBeNull();
  });
});

// The certificate's name is one fact, and the save confirmation judges the file
// that landed against the name the steps beside it run on. A second copy of that
// name would decide this message against steps built from the other one.
test("a renamed save is confirmed against the name the steps here actually use", async () => {
  draw({ onSaveCa: vi.fn(async () => "/tmp/my-ca.pem") });
  fireEvent.click(within(screen.getByTestId("precondition-trust")).getByText("set it up by hand"));
  fireEvent.click(screen.getByRole("button", { name: "save…" }));

  const note = await screen.findByTestId("save-ca-status");
  expect(note).toHaveTextContent("saved as my-ca.pem");
  expect(note).toHaveTextContent(`the steps here name ${setup.CA_FILENAME}`);
  expect(screen.getByTestId("precondition-trust")).toHaveTextContent(setup.SAVE_CA_STEP);
});

describe("the way out", () => {
  it("is absent until capture has been proven at least once", () => {
    draw();
    expect(screen.queryByRole("button", { name: COPY.back })).toBeNull();
  });

  it("stays open once it has, even with a precondition regressed", () => {
    const dispatch = vi.fn();
    draw({ state: { ...onboarding, everProven: true }, trustProbe: "untrusted", dispatch });
    fireEvent.click(screen.getByRole("button", { name: COPY.back }));
    expect(dispatch).toHaveBeenCalledWith({ t: "exit" });
  });
});

// E-REDESIGN's nit: the one-second tick re-rendered the whole step list, where
// only the traffic verdict depends on it. `setupFor` is the work the
// preconditions zone does on every render of the screen, so a tick that leaves
// it uncalled is a tick that re-rendered the verdict and nothing else.
describe("the clock reaches the traffic verdict and nothing else", () => {
  beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: true }));
  afterEach(() => vi.useRealTimers());

  it("updates the verdict without re-running the instructions", () => {
    const resolved = vi.mocked(setup.setupFor);
    draw({ traffic: { ...NO_TRAFFIC, lastDecryptedAt: Date.now(), ever: true } });
    expect(zone("proven")).toBeInTheDocument();

    resolved.mockClear();
    act(() => vi.advanceTimersByTime(RECENT_MS + 2_000));

    expect(zone("proven")).toBeNull();
    expect(zone("lead-verdict")).toHaveTextContent("nothing is arriving now");
    expect(resolved).not.toHaveBeenCalled();
  });
});

test("the column scrolls whole — no zone holds an inner scroller", () => {
  const { container } = draw();
  const scrollers = container.querySelectorAll("[class*='overflow-auto'],[class*='overflow-y-auto']");
  expect(scrollers).toHaveLength(1);
  expect(scrollers[0]).toBe(container.firstElementChild);
});
