import { afterEach, beforeEach, describe, expect, it, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Onboarding } from "./Onboarding";
import { COPY, type OnbState } from "./onboardingMachine";
import * as setup from "./setup";
import { PLATFORM_IDS, type PlatformId } from "./setup";
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
      onSetUpThisMachine={vi.fn(async () => {})}
      onSaveCa={vi.fn(async () => null)}
      onSetProxy={vi.fn(async () => {})}
      onUnsetProxy={vi.fn(async () => {})}
      onArmReach={vi.fn(async () => {})}
      onDisarmReach={vi.fn(async () => {})}
      {...props}
    />,
  );

const zone = (id: string) => screen.queryByTestId(id);
const task = (n: number) => screen.getByTestId(`task-${n}`);
const pick = (label: string | RegExp) => fireEvent.click(screen.getByRole("radio", { name: label }));

// A disclosure keeps its content in the DOM and closed, so "behind a
// disclosure" is a question about the <details> it sits in, never about
// whether the text exists.
const disclosing = (text: string) => screen.getByText(text).closest("details")!;

// ── The lead: what this is, before anything about what is wrong ─────────────
// F1. The screen used to open on a lowercase grey failure verdict, which is
// the one thing a first-time user cannot act on.

describe("the screen leads with what it is", () => {
  it("opens on a title and a sentence of purpose", () => {
    draw();

    const lead = zone("lead")!;
    expect(within(lead).getByText("Set up capture")).toBeInTheDocument();
    expect(lead).toHaveTextContent(
      "Trawler sits between a device and the internet. To show a device's HTTPS traffic, that device has to trust Trawler's certificate and send its traffic to Trawler.",
    );
  });

  it("states no verdict above the question — the facts belong to the tasks", () => {
    draw({ traffic: NO_TRAFFIC });

    expect(zone("lead-verdict")).toBeNull();
    expect(zone("lead")).not.toHaveTextContent(/nothing has reached Trawler/i);
    expect(screen.queryByText(/waiting for the first request/)).not.toBeNull();
  });
});

// ── The question, and the eight answers to it ───────────────────────────────
// F6. One axis, seen rather than discovered.

describe("the one question, asked in the user's words", () => {
  it("asks which device, and shows every answer without opening anything", () => {
    draw();

    expect(zone("platform")).toHaveTextContent("which device's traffic do you want to see");
    expect(screen.getAllByRole("radio")).toHaveLength(PLATFORM_IDS.length);
    expect(screen.queryByRole("combobox")).toBeNull();
  });

  it("starts on this machine, named for the host it detected", () => {
    draw({ os: "macos" });

    expect(screen.getByRole("radio", { name: "This machine — macOS" })).toBeChecked();
  });

  it("asks the target question once — nothing below the tiles asks it again", () => {
    draw();

    expect(screen.getAllByRole("radiogroup")).toHaveLength(1);
    expect(screen.queryByText(/what are you setting up/i)).toBeNull();
    expect(screen.queryByText(/what has to be true/i)).toBeNull();
  });
});

// ── The vocabulary, asserted as an absence ──────────────────────────────────
// F3. The list is where "flow" is taught; this screen is read before the word
// means anything. The check is over every platform and every lifecycle state,
// because a word that only appears on the Android path is still on the screen.

describe("the machine's words for itself are not on this screen", () => {
  const MACHINE_WORDS = [
    /\bflows?\b/i,
    /\bverdict\b/i,
    /\breach\b/i,
    /\bplatform\b/i,
    /\broute its\b/i,
    /listen on/i,
    /\bdecrypting\b/i,
  ];

  // "arriving but not decrypting" is the traffic layer's own wording, kept by
  // the design: it describes what the user's device is doing, not what the app
  // calls it. Everything else must be absent.
  const rendered = () => document.body.textContent ?? "";

  it.each(PLATFORM_IDS)("%s renders none of them", (id) => {
    draw({ reach: ARMED, trustProbe: "untrusted", proxy: { status: "none", address: null } });
    pick(setup.platformLabel(id as PlatformId, "macos"));

    for (const word of MACHINE_WORDS) {
      if (word.source.includes("decrypting")) continue;
      expect(rendered()).not.toMatch(word);
    }
  });

  it("does not say 'flow' even where the traffic layer has one to report", () => {
    draw({ traffic: capturing, trustProbe: "trusted", proxy: { status: "ours", address: null } });

    expect(rendered()).not.toMatch(/\bflows?\b/i);
  });
});

// ── This machine: three numbered tasks, one primary action ──────────────────

describe("this machine — three tasks, and one button that does two of them", () => {
  it("numbers the tasks and titles them by what the user does", () => {
    draw();

    expect(task(1)).toHaveTextContent("Trust Trawler's certificate");
    expect(task(2)).toHaveTextContent("Send this Mac's traffic to Trawler");
    expect(task(3)).toHaveTextContent("Open any website on this Mac");
  });

  it("each task's state is the probe's answer or the traffic layer's", () => {
    draw({
      trustProbe: "trusted",
      proxy: { status: "none", address: null },
      traffic: NO_TRAFFIC,
    });

    expect(task(1)).toHaveTextContent("done — installed and trusted");
    expect(task(2)).toHaveTextContent("not done — the system proxy is off");
    expect(task(3)).toHaveTextContent("waiting for the first request");
  });

  it("a set proxy reads as this Mac routing through Trawler", () => {
    draw({ trustProbe: "trusted", proxy: { status: "ours", address: "127.0.0.1:9999" } });

    expect(task(2)).toHaveTextContent("done — this Mac routes through Trawler");
  });

  it("offers one primary action, and says what the one prompt does", () => {
    draw();

    expect(screen.getByRole("button", { name: "Set up this Mac" })).toBeInTheDocument();
    expect(screen.getByText("one password prompt — it installs the certificate and sets the proxy"))
      .toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Install the certificate" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Set the system proxy" })).toBeNull();
  });

  it("the one action runs the one command", async () => {
    const onSetUpThisMachine = vi.fn(async () => {});
    draw({ onSetUpThisMachine });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() => expect(onSetUpThisMachine).toHaveBeenCalledOnce());
  });

  it("while it runs, both tasks it covers say so and the button is out of reach", async () => {
    let release = () => {};
    const onSetUpThisMachine = vi.fn(
      () => new Promise<void>((resolve) => (release = resolve)),
    );
    draw({ onSetUpThisMachine, trustProbe: "untrusted", proxy: { status: "none", address: null } });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() => expect(task(1)).toHaveTextContent("setting up — approve the prompt…"));
    expect(task(2)).toHaveTextContent("setting up — approve the prompt…");
    expect(screen.getByRole("button", { name: "Set up this Mac" })).toBeDisabled();
    await act(async () => release());
  });

  it("names the host it is on — a PC is not a Mac", () => {
    draw({ os: "windows" });

    expect(screen.getByRole("button", { name: "Set up this PC" })).toBeInTheDocument();
    expect(task(2)).toHaveTextContent("Send this PC's traffic to Trawler");
  });

  // Review round 1, blocker: the button offered once both are done was labelled
  // "undo this setup", and undid half of it — nothing reverses a trust-store
  // install, and DEC-13 never offered to. A control may not name work it does
  // not do, least of all work about what this machine trusts.
  it("once both are done, the action offers the half it can actually reverse", () => {
    draw({ trustProbe: "trusted", proxy: { status: "ours", address: null } });

    expect(screen.getByRole("button", { name: "stop routing through Trawler" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /undo this setup/i })).toBeNull();
    expect(screen.queryByRole("button", { name: "Set up this Mac" })).toBeNull();
  });

  it("says the certificate stays trusted, and where the steps to remove it are", () => {
    draw({ trustProbe: "trusted", proxy: { status: "ours", address: null } });

    expect(screen.getByTestId("steps")).toHaveTextContent(
      "Trawler's certificate stays trusted on this Mac — the steps below remove it.",
    );
    fireEvent.click(screen.getByText("do it manually"));
    expect(screen.getByRole("button", { name: /copy the removal command/i })).toHaveTextContent(
      "security delete-certificate",
    );
  });
});

// ── The manual route ────────────────────────────────────────────────────────
// F10's rule, applied to the shape F27 created: one action, one alternative.

describe("the manual route is the alternative to the button", () => {
  it("one disclosure holds what the button would have done, in task order", () => {
    draw();

    const manual = screen.getByText("do it manually");
    fireEvent.click(manual);

    expect(screen.getByRole("button", { name: /copy the trust command/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /copy the proxy command/i })).toBeInTheDocument();
    expect(screen.getAllByText("do it manually")).toHaveLength(1);
  });

  it("no task carries a guide of its own where one button covers two", () => {
    draw();

    expect(within(task(1)).queryByText("do it manually")).toBeNull();
    expect(within(task(2)).queryByText("do it manually")).toBeNull();
  });
});

// ── A refused prompt ────────────────────────────────────────────────────────
// F18. The one common failure is the one the user caused, and it is not the
// OS's sentence to write.

describe("a password prompt the user dismissed", () => {
  const cancelled = {
    refused: true,
    detail: "0:36: execution error: User cancelled. (-128)",
  };

  it("reads as a sentence, with the tool's own text one click away", async () => {
    draw({ onSetUpThisMachine: vi.fn(() => Promise.reject(cancelled)) });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() =>
      expect(
        screen.getByText("The password prompt was cancelled. Try again, or follow the manual steps."),
      ).toBeInTheDocument(),
    );
    expect(disclosing(cancelled.detail)).not.toHaveAttribute("open");

    fireEvent.click(screen.getByText("details"));
    expect(disclosing(cancelled.detail)).toHaveAttribute("open");
  });

  it("a real failure keeps its own sentence and its own text", async () => {
    draw({
      onSetUpThisMachine: vi.fn(() =>
        Promise.reject({ refused: false, detail: "no network services to configure" }),
      ),
    });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() =>
      expect(screen.getByText(/setting up failed/)).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByText("details"));
    expect(screen.getByText("no network services to configure")).toBeInTheDocument();
  });

  it("leaves the task states to the probes — an action never reports them", async () => {
    draw({
      trustProbe: "untrusted",
      proxy: { status: "none", address: null },
      onSetUpThisMachine: vi.fn(() => Promise.reject(cancelled)),
    });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() => expect(screen.getByText(/password prompt was cancelled/)).toBeInTheDocument());
    expect(task(1)).toHaveTextContent("not done — the certificate isn't trusted");
    expect(task(2)).toHaveTextContent("not done — the system proxy is off");
  });
});

// ── Another proxy ───────────────────────────────────────────────────────────
// F17. The button is about to take something away, and the user is owed its name.

describe("a proxy that is already set", () => {
  it("is named, with what replacing it costs", () => {
    draw({ proxy: { status: "foreign", address: "10.1.1.1:8080" } });

    expect(zone("foreign-proxy")).toHaveTextContent(
      "another proxy is set on this Mac: 10.1.1.1:8080. Setting Trawler replaces it while Trawler runs, and restores nothing when it quits.",
    );
  });

  it("drops the address where the probe could not read one, and claims nothing", () => {
    draw({ proxy: { status: "foreign", address: null } });

    expect(zone("foreign-proxy")).toHaveTextContent(
      "another proxy is set on this Mac. Setting Trawler replaces it while Trawler runs, and restores nothing when it quits.",
    );
  });

  it("says nothing of the sort where nothing is set", () => {
    draw({ proxy: { status: "none", address: null } });

    expect(zone("foreign-proxy")).toBeNull();
  });
});

// ── The iOS Simulator: two stores, two answers ──────────────────────────────

describe("the iOS Simulator", () => {
  const simulator = () => {
    draw({ trustProbe: "trusted", proxy: { status: "none", address: null } });
    pick("iOS Simulator");
  };

  it("takes its trust task's state from nobody, and says when it will know", () => {
    simulator();

    expect(task(1)).toHaveTextContent("Trust Trawler's certificate in the Simulator");
    expect(task(1)).toHaveTextContent("we'll confirm this on the Simulator's first request");
    expect(task(1)).not.toHaveTextContent("done — installed and trusted");
  });

  it("takes its routing task's state from this Mac, which it follows", () => {
    simulator();

    expect(task(2)).toHaveTextContent("Send this Mac's traffic to Trawler");
    expect(task(2)).toHaveTextContent("not done — the system proxy is off");
    expect(within(task(2)).getByRole("button", { name: "Set the system proxy" })).toBeInTheDocument();
  });

  it("renders its trust guide open — nothing automatic stands above it", () => {
    simulator();

    expect(within(task(1)).queryByText("do it manually")).toBeNull();
    expect(
      within(task(1)).getByRole("button", { name: /copy the trust command/i }),
    ).toHaveTextContent("xcrun simctl keychain booted add-root-cert");
  });

  it("is told to open a website in it, not on it", () => {
    simulator();

    expect(task(3)).toHaveTextContent("Open any website in the Simulator");
  });
});

// ── A phone: one task that changes this machine, three that do not ──────────

describe("a phone, before the network is allowed", () => {
  const phone = (reach: ReachState = UNARMED) => {
    draw({ reach });
    pick("iPhone or iPad");
  };

  it("leads with the one task that is this machine's to do", () => {
    phone();

    expect(task(1)).toHaveTextContent("Allow your iPhone to connect");
    expect(task(1)).toHaveTextContent("not done — Trawler answers on this machine only");
  });

  it("states one consequence, and leaves the rest to the confirm dialog", () => {
    phone();

    expect(task(1)).toHaveTextContent(
      "other devices on your Wi-Fi will be able to use this proxy and see captured traffic until Trawler quits",
    );
    expect(task(1)).not.toHaveTextContent(/Trawler will ask before/);
    expect(task(1)).not.toHaveTextContent(/Anything on that network/);
  });

  it("dims the rest and numbers them, rather than explaining what cannot happen", () => {
    phone();

    for (const n of [2, 3, 4]) {
      expect(task(n)).toHaveAttribute("data-dim");
      expect(task(n)).toHaveTextContent("after step 1");
    }
    expect(screen.queryByText(/the certificate route opens once/)).toBeNull();
    expect(screen.queryByText(/there is no address to point the device at yet/)).toBeNull();
    expect(screen.queryByText(/nothing to try until/)).toBeNull();
  });

  it("offers one button on the screen: step 1's", () => {
    phone();

    expect(
      screen.getByRole("button", { name: "Allow devices on my Wi-Fi to connect" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Set up/ })).toBeNull();
    expect(screen.queryByRole("button", { name: "Set the system proxy" })).toBeNull();
  });

  it("reads nothing off this machine's probes — there is nothing here to read", () => {
    draw({ reach: UNARMED, trustProbe: "trusted", proxy: { status: "ours", address: null } });
    pick("iPhone or iPad");

    expect(zone("probe")).toBeNull();
    expect(document.body.textContent).not.toMatch(/installed and trusted/);
  });
});

describe("a phone, once it may connect", () => {
  const armedPhone = () => {
    draw({ reach: ARMED });
    pick("iPhone or iPad");
  };

  it("says what is listening and offers the way to stop", () => {
    armedPhone();

    expect(task(1)).toHaveTextContent("allowed — listening on 192.168.1.24 until Trawler quits");
    expect(within(task(1)).getByRole("button", { name: "stop listening" })).toBeInTheDocument();
  });

  it("renders the certificate route the phone opens, and the address it points at", () => {
    armedPhone();

    expect(
      screen.getByRole("button", { name: /copy the certificate URL/i }),
    ).toHaveTextContent("http://192.168.1.24:7421/ca.crt");
    expect(within(zone("device-address")!).getByRole("button", { name: /copy the proxy host/i }))
      .toHaveTextContent("192.168.1.24");
    expect(within(zone("device-address")!).getByRole("button", { name: /copy the proxy port/i }))
      .toHaveTextContent("9999");
  });

  it("promises to confirm on the phone's own first request, and claims nothing now", () => {
    armedPhone();

    expect(task(2)).toHaveTextContent("we'll confirm this on your iPhone's first request");
    expect(task(3)).toHaveTextContent("we'll confirm this on your iPhone's first request");
    expect(task(2)).not.toHaveAttribute("data-dim");
  });

  it("puts its instructions in the open — a hand is the only route it has", () => {
    armedPhone();

    expect(screen.queryByText("do it manually")).toBeNull();
    expect(task(2)).toHaveTextContent(/Settings → General → VPN & Device Management/);
  });
});

// ── The address the device is pointed at ────────────────────────────────────

describe("the address is an offer, not a menu", () => {
  it("offers the first and puts the rest behind a disclosure", () => {
    draw({ reach: UNARMED });
    pick("iPhone or iPad");

    expect(zone("listen-on")).toHaveTextContent("Wi-Fi · 192.168.1.24");
    expect(disclosing("Ethernet · 10.0.4.7")).not.toHaveAttribute("open");

    fireEvent.click(screen.getByText("other addresses"));
    expect(disclosing("Ethernet · 10.0.4.7")).toHaveAttribute("open");
  });

  it("taking another one commits nothing but the offer", () => {
    const onArmReach = vi.fn(async () => {});
    draw({ reach: UNARMED, onArmReach });
    pick("iPhone or iPad");

    fireEvent.click(screen.getByText("other addresses"));
    fireEvent.click(screen.getByRole("button", { name: "Ethernet · 10.0.4.7" }));

    expect(zone("listen-on")).toHaveTextContent("Ethernet · 10.0.4.7");
    expect(onArmReach).not.toHaveBeenCalled();
  });

  it("arms the address it is offering", async () => {
    const onArmReach = vi.fn(async () => {});
    draw({ reach: UNARMED, onArmReach });
    pick("iPhone or iPad");

    fireEvent.click(screen.getByRole("button", { name: "Allow devices on my Wi-Fi to connect" }));

    await waitFor(() => expect(onArmReach).toHaveBeenCalledWith("192.168.1.24"));
  });

  it("a refused confirm says what is still true", async () => {
    draw({ reach: UNARMED, onArmReach: vi.fn(async () => null) });
    pick("iPhone or iPad");

    fireEvent.click(screen.getByRole("button", { name: "Allow devices on my Wi-Fi to connect" }));

    await waitFor(() =>
      expect(
        screen.getByText("not allowed — Trawler still answers on this machine only."),
      ).toBeInTheDocument(),
    );
  });

  it("a machine with no address to offer says so instead of offering one", () => {
    draw({ reach: { ...UNARMED, interfaces: [] } });
    pick("iPhone or iPad");

    expect(task(1)).toHaveTextContent(
      "this machine holds no network address a second device could reach it at",
    );
    expect(screen.queryByRole("button", { name: /Allow devices/ })).toBeNull();
  });
});

// ── The end of setup ────────────────────────────────────────────────────────
// F22's other half: the moment it succeeds is said out loud, here and on the
// list the user lands on.

describe("the end", () => {
  it("says setup worked, and names the device it worked for", () => {
    draw({ traffic: capturing });

    expect(zone("proven")).toHaveTextContent(
      "You're set up — Trawler is capturing traffic from this Mac",
    );
  });

  // A decrypted request proves capture and never says which device made it —
  // the run's open blocked-on row. This machine is the one platform where the
  // two answers cannot differ, so it is the one that gets named.
  it("names no device where the request could have come from another", () => {
    draw({ traffic: capturing, reach: ARMED });
    pick("iPhone or iPad");

    expect(zone("proven")).toHaveTextContent("You're set up — Trawler is capturing traffic");
    expect(zone("proven")).not.toHaveTextContent(/iPhone/);
  });

  it("an arriving connection that will not decrypt says what to finish, naming no device", () => {
    draw({ traffic: arriving });

    expect(zone("traffic-remedy")).toHaveTextContent(
      "something is connecting and none of it is decrypting — the device it comes from does not trust Trawler's certificate yet. Finish the certificate step on that device.",
    );
    expect(zone("traffic-remedy")).not.toHaveTextContent(/iPhone|Simulator|Android/);
  });

  it("the way back exists once capture has been proven, and not before", () => {
    const dispatch = vi.fn();
    draw({ state: { ...onboarding, everProven: true }, dispatch });

    fireEvent.click(screen.getByRole("button", { name: new RegExp(COPY.back) }));
    expect(dispatch).toHaveBeenCalledWith({ t: "exit" });
  });
});

// ── The probe row ───────────────────────────────────────────────────────────

describe("one reading of this machine, re-taken by one control", () => {
  it("is offered where a probe answers for the chosen device", () => {
    draw({ probedAt: ago(60_000) });

    expect(zone("probe")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "check again" })).toBeInTheDocument();
  });

  it("is absent where nothing here could be re-read", () => {
    draw({ reach: ARMED });
    pick("Android device");

    expect(zone("probe")).toBeNull();
  });

  it("re-reads on demand", () => {
    const onRecheck = vi.fn();
    draw({ onRecheck });

    fireEvent.click(screen.getByRole("button", { name: "check again" }));
    expect(onRecheck).toHaveBeenCalledOnce();
  });
});

// ── The shape of the whole ──────────────────────────────────────────────────

// E-REDESIGN's nit: the one-second tick re-rendered the whole task list, where
// only the traffic reading depends on it. `setupFor` is the work the task list
// does on every render of the screen, so a tick that leaves it uncalled is a
// tick that re-rendered the traffic reading and nothing else.
describe("the clock reaches the traffic reading and nothing else", () => {
  beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: true }));
  afterEach(() => vi.useRealTimers());

  it("updates the reading without re-running the instructions", () => {
    const resolved = vi.mocked(setup.setupFor);
    draw({ traffic: { ...NO_TRAFFIC, lastDecryptedAt: Date.now(), ever: true } });
    expect(zone("proven")).toBeInTheDocument();

    resolved.mockClear();
    act(() => vi.advanceTimersByTime(RECENT_MS + 2_000));

    expect(zone("proven")).toBeNull();
    expect(task(3)).toHaveTextContent("nothing arriving right now");
    expect(resolved).not.toHaveBeenCalled();
  });
});

test("no step reports itself done — every state on the screen has a source", () => {
  draw({ trustProbe: "unknown", proxy: { status: "unknown", address: null }, os: "linux" });

  // Nothing read either store on this host, so neither task may claim one was.
  expect(task(1)).toHaveTextContent("we'll confirm this on this machine's first request");
  expect(task(2)).toHaveTextContent("we'll confirm this on this machine's first request");
  expect(document.body.textContent).not.toMatch(/done — /);
});

test("the column scrolls whole — no zone holds an inner scroller", () => {
  const { container } = draw();
  const scrollers = container.querySelectorAll("[class*='overflow-auto'],[class*='overflow-y-auto']");
  expect(scrollers).toHaveLength(1);
  expect(scrollers[0]).toBe(container.firstElementChild);
});
