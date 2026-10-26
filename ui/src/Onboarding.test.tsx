import { afterEach, beforeEach, describe, expect, it, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Onboarding, useRouteMemory } from "./Onboarding";
import { COPY, type OnbState } from "./onboardingMachine";
import * as setup from "./setup";
import { PLATFORM_IDS, type PlatformId } from "./setup";
import { clock, NO_TRAFFIC, RECENT_MS, TUNNEL_GRACE_MS, type EmulatorReading, type Traffic } from "./verdicts";
import type { RequestRoute } from "./ownRequest";
import type { ReachState } from "./api";

vi.mock("./setup", async (importOriginal) => {
  const mod = await importOriginal<typeof import("./setup")>();
  return { ...mod, setupFor: vi.fn(mod.setupFor) };
});

const onboarding: OnbState = {
  phase: "onboarding",
  step: "trust-ca",
  everProven: false,
  since: null,
  chosen: "this",
  another: false,
};
const ADDR = "127.0.0.1:9999";

const INTERFACES = [
  { name: "Wi-Fi", address: "192.168.1.24" },
  { name: "Ethernet", address: "10.0.4.7" },
];

const UNARMED: ReachState = {
  armed: false,
  proxy: null,
  api: null,
  ca_urls: null,
  interfaces: INTERFACES,
};

const ARMED: ReachState = {
  armed: true,
  proxy: "192.168.1.24:9999",
  api: "192.168.1.24:7421",
  ca_urls: {
    pem: "http://192.168.1.24:7421/ca.pem",
    cer: "http://192.168.1.24:7421/ca.cer",
    mobileconfig: "http://192.168.1.24:7421/ca.mobileconfig",
  },
  interfaces: INTERFACES,
};

const ago = (ms: number): number => Date.now() - ms;
const capturing: Traffic = { ...NO_TRAFFIC, lastDecryptedAt: ago(1_000) };
// This machine's proxy does not point here, so nothing it sends can be mistaken
// for another device's (DEC-63).
const arriving: Traffic = { ...NO_TRAFFIC, lastTunnelAt: ago(5_000), openTunnelAt: ago(5_000) };

type Props = Partial<Parameters<typeof Onboarding>[0]>;

// App holds what the shell last did to the proxy (S5); the host holds it the same way.
function Host(props: Props) {
  const [routeAct, onRouteAct] = useRouteMemory(props.proxy?.status ?? "unknown");
  return (
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
      routeAct={routeAct}
      onRouteAct={onRouteAct}
      {...props}
    />
  );
}

const screenAt = (props: Props) => <Host {...props} />;

const draw = (props: Props = {}) => render(screenAt(props));

const QUESTION = "which device's traffic do you want to see";
const MANUAL = "do it manually";

const zone = (id: string) => screen.queryByTestId(id);
const task = (n: number) => screen.getByTestId(`task-${n}`);
const picker = () => screen.getByRole("button", { name: QUESTION });
const pick = (label: string | RegExp) => {
  fireEvent.click(picker());
  fireEvent.click(screen.getByRole("option", { name: label }));
};

// A disclosure keeps its content in the DOM and closed, so "behind a
// disclosure" is a question about the <details> it sits in, never about
// whether the text exists.
const disclosing = (text: string) => screen.getByText(text).closest("details")!;
const open = (text: string) => disclosing(text).hasAttribute("open");

// A command is a block now: the copy control carries the command's name and
// the command itself sits beside it, so the block is what holds both.
const commandNamed = (label: string) =>
  screen.getByRole("button", { name: label }).closest("div")!;

// ── The lead: what this is, before anything about what is wrong ─────────────
// F1. The screen used to open on a lowercase grey failure verdict, which is
// the one thing a first-time user cannot act on.

// ── The way in: which device this session is for (R6 · Q2) ──────────────────
// "Set up another device" preselected this machine, the one already set up.

describe("the question opens on the way in", () => {
  const another: OnbState = { ...onboarding, everProven: true, another: true };

  it("left open for another device over this machine: no task, no re-read, the way back", () => {
    draw({ state: another });

    expect(picker()).toHaveTextContent("choose a device");
    expect(zone("lead")).not.toBeNull();
    expect(zone("steps")).toBeNull();
    expect(zone("probe")).toBeNull();
    expect(zone("proven")).toBeNull();
    expect(zone("back")).not.toBeNull();
  });

  it("answered, it draws that platform's tasks with the first guide open, and says what was chosen", () => {
    const dispatch = vi.fn();
    draw({ state: another, dispatch });

    pick("Android emulator");

    expect(zone("steps")).not.toBeNull();
    expect(task(1).querySelector("details[open]")).not.toBeNull();
    expect(dispatch).toHaveBeenCalledWith({ t: "choose", platform: "android-sim" });
  });

  it("opens on the last device chosen", () => {
    draw({ state: { ...another, chosen: "ios" } });
    expect(picker()).toHaveTextContent("iPhone or iPad");
    expect(zone("steps")).not.toBeNull();
  });

  it("left open, still says this session is capturing, naming no device", () => {
    draw({ state: another, traffic: { ...NO_TRAFFIC, lastDecryptedAt: Date.now() } });
    expect(zone("proven")).toHaveTextContent("You're set up — Trawler is capturing traffic");
    expect(zone("proven")).not.toHaveTextContent(/from /);
  });
});

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
  it("asks which device, and every device is an answer the control offers", () => {
    draw();

    expect(zone("platform")).toHaveTextContent(QUESTION);

    fireEvent.click(picker());
    expect(screen.getAllByRole("option")).toHaveLength(PLATFORM_IDS.length);
    for (const id of PLATFORM_IDS) {
      expect(
        screen.getByRole("option", { name: setup.platformLabel(id, "macos") }),
      ).toBeInTheDocument();
    }
  });

  it.each(["windows", "linux", "other"] as const)(
    "on %s offers no iOS Simulator, which runs beside macOS only (R8)",
    (os) => {
      draw({ os });

      fireEvent.click(picker());
      expect(screen.queryByRole("option", { name: "iOS Simulator" })).toBeNull();
      expect(screen.getByRole("option", { name: "Android emulator" })).toBeInTheDocument();
      expect(screen.getAllByRole("option")).toHaveLength(PLATFORM_IDS.length - 1);
    },
  );

  it("starts on this machine, named for the host it detected, and says so unopened", () => {
    draw({ os: "macos" });

    expect(picker()).toHaveTextContent("This machine — macOS");
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("the answer is taken from the list, and the control then carries it", () => {
    draw();

    pick("iPhone or iPad");

    expect(picker()).toHaveTextContent("iPhone or iPad");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(task(1)).toHaveTextContent("Allow your iPhone to connect");
  });

  it("holds its width whatever the answer is, so the row beside it does not move", () => {
    draw();
    const box = picker().parentElement!;

    expect(box.className).toContain("w-picker");
    pick("Another Ubuntu machine");
    expect(picker().parentElement).toBe(box);
    expect(box.className).toContain("w-picker");
  });

  it("asks the target question once — nothing below it asks again", () => {
    draw();

    expect(screen.getAllByRole("button", { name: QUESTION })).toHaveLength(1);
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

  // "not working — arriving but not decrypting" is the traffic layer's own wording, kept by
  // the design: it describes what the user's device is doing, not what the app
  // calls it. Everything else must be absent.
  const rendered = () => document.body.textContent ?? "";

  it.each(PLATFORM_IDS)("%s renders none of them", (id) => {
    draw({ reach: ARMED, trustProbe: "absent", proxy: { status: "none", address: null } });
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
    expect(task(3)).toHaveTextContent("not done — waiting for the first request");
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

  // F19: the tasks say what is happening, the note says what the user must do,
  // and neither says the other's sentence.
  it("while it runs, both tasks it covers say so, the prompt is named once, and the button is out of reach", async () => {
    let release = () => {};
    const onSetUpThisMachine = vi.fn(
      () => new Promise<void>((resolve) => (release = resolve)),
    );
    draw({ onSetUpThisMachine, trustProbe: "absent", proxy: { status: "none", address: null } });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() => expect(screen.getByTestId("task-1-state")).toHaveTextContent(/^⟳?setting up$/));
    expect(screen.getByTestId("task-2-state")).toHaveTextContent(/^⟳?setting up$/);
    expect(screen.getAllByText(/approve the prompt/)).toHaveLength(1);
    expect(screen.getByTestId("set-up-status")).toHaveTextContent("approve the prompt that just opened");
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

  // S6: the reversal was the one action a reading in flight did not hold.
  it("the reversal waits for a reading in flight, like every other action", () => {
    draw({ trustProbe: "trusted", proxy: { status: "ours", address: null }, probing: true });

    expect(screen.getByRole("button", { name: "stop routing through Trawler" })).toBeDisabled();
  });

  it("says what the reversal leaves behind, and where the steps for it are", () => {
    draw({ trustProbe: "trusted", proxy: { status: "ours", address: null } });

    expect(task(2)).toHaveTextContent(
      "this turns the proxy off and nothing else — Trawler's certificate stays trusted, and step 1 has the steps to remove it.",
    );
    fireEvent.click(within(task(1)).getByText("to remove it later"));
    expect(within(task(1)).getByRole("button", { name: "the removal command" }).closest("div"))
      .toHaveTextContent("security delete-certificate");
  });

  // Review round 3, important: DEC-13's inherited rule is that an action which
  // changed this host may not become irreversible because a probe has not
  // answered yet. The reversal read the probes alone, so the window between a
  // successful run and the re-read it triggers had no way out — and a probe
  // that never answers had none at all.
  it("offers the way out as soon as the action resolved, before any probe confirms it", async () => {
    draw({
      trustProbe: "unknown",
      proxy: { status: "unknown", address: null },
      onSetUpThisMachine: vi.fn(async () => {}),
    });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "stop routing through Trawler" }),
      ).toBeInTheDocument(),
    );
    // And it does not claim the tasks are done — that is still the probes' word.
    expect(task(1)).toHaveTextContent("can't check here — the request in step 3 confirms it");
  });

  // Review round 4, important: the way out was kept alive by the action that
  // opened it, and that latch never cleared — so once the user took it, the
  // screen went on offering to turn off a proxy that was already off, beside a
  // button offering to set it up again.
  // Review round 5: round 4's term gated only the arm the action opened, so on
  // the path a real user takes — the probe has already read "ours" — a
  // succeeded reversal still waited for the next probe before the control went
  // away. The rule is that a reversal which succeeded closes the way out
  // whichever arm opened it.
  it("retires the way out on the path the probe opened, without waiting for the next read", async () => {
    const onUnsetProxy = vi.fn(async () => {});
    draw({
      trustProbe: "trusted",
      proxy: { status: "ours", address: null },
      onUnsetProxy,
    });

    fireEvent.click(screen.getByRole("button", { name: "stop routing through Trawler" }));

    await waitFor(() => expect(onUnsetProxy).toHaveBeenCalledOnce());
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "stop routing through Trawler" })).toBeNull(),
    );
  });

  it("retires the way out once the reversal itself has succeeded", async () => {
    const onUnsetProxy = vi.fn(async () => {});
    const props: Props = {
      trustProbe: "unknown",
      proxy: { status: "unknown", address: null },
      onSetUpThisMachine: vi.fn(async () => {}),
      onUnsetProxy,
    };
    const { rerender } = draw(props);

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "stop routing through Trawler" })).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: "stop routing through Trawler" }));
    await waitFor(() => expect(onUnsetProxy).toHaveBeenCalledOnce());

    // The probe catches up: the proxy this app set is no longer set.
    rerender(screenAt({ ...props, proxy: { status: "none", address: null } }));

    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "stop routing through Trawler" })).toBeNull(),
    );
    expect(screen.getByRole("button", { name: "Set up this Mac" })).toBeInTheDocument();
  });

  it("keeps the way out while a reversal that failed leaves the proxy set", async () => {
    draw({
      trustProbe: "unknown",
      proxy: { status: "unknown", address: null },
      onSetUpThisMachine: vi.fn(async () => {}),
      onUnsetProxy: vi.fn(() => Promise.reject({ refused: false, detail: "no services" })),
    });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "stop routing through Trawler" })).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: "stop routing through Trawler" }));

    await waitFor(() => expect(screen.getByText(/couldn't turn the proxy off/)).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "stop routing through Trawler" })).toBeInTheDocument();
  });

  it("keeps offering the action until the probes say the work is done", async () => {
    draw({
      trustProbe: "unknown",
      proxy: { status: "unknown", address: null },
      onSetUpThisMachine: vi.fn(async () => {}),
    });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "stop routing through Trawler" })).toBeInTheDocument(),
    );
    expect(screen.getByRole("button", { name: "Set up this Mac" })).toBeInTheDocument();
  });
});

// ── The manual route ────────────────────────────────────────────────────────
// F10's rule, applied to the shape F27 created: one action, one alternative.

describe("each task holds its own route, under the one button", () => {
  it("the certificate's steps are task 1's and the proxy's are task 2's", () => {
    draw();

    expect(within(task(1)).getByText(MANUAL)).toBeInTheDocument();
    expect(within(task(2)).getByText(MANUAL)).toBeInTheDocument();
    expect(within(task(1)).getByRole("button", { name: "the trust command" })).toBeInTheDocument();
    expect(within(task(1)).queryByRole("button", { name: "the proxy command" })).toBeNull();
    expect(within(task(2)).getByRole("button", { name: "the proxy command" })).toBeInTheDocument();
    expect(within(task(2)).queryByRole("button", { name: "the trust command" })).toBeNull();
  });

  it("stays shut until it is asked for — the button is the route, these are the other one", () => {
    draw();

    expect(open("Save Trawler's certificate as trawler-ca.pem")).toBe(false);
    expect(open("Run — this sets the Wi-Fi service; change the name for Ethernet:")).toBe(false);
  });

  it("keeps each task's asides as the kind of aside each one was", () => {
    draw();
    fireEvent.click(within(task(1)).getByText(MANUAL));

    expect(within(task(1)).queryByTestId("guide-note")).toBeNull();
    expect(open("Or double-click the file and set the Trawler CA to Always Trust.")).toBe(false);
    expect(within(task(1)).getByText("to remove it later")).toBeInTheDocument();
    expect(within(task(2)).getByText("to turn it off later")).toBeInTheDocument();
  });

  // W6 C1: Linux keeps three certificate stores, and the browsers read neither
  // of the other two, so each store is a step - browsers first - and none is a
  // note or another way. Firefox reads the system's store on macOS and Windows.
  it("gives each of Linux's three stores a step, browsers first, and names Firefox on Linux only", () => {
    draw({ os: "linux" });
    const firefox = within(task(1)).getByText(/^In Firefox: .* → Authorities → Import → trawler-ca\.pem/);
    const [chrome] = within(task(1)).getAllByText("For Chrome, run:");
    const [system] = within(task(1)).getAllByText("For curl and the system's tools, run:");

    expect(firefox.compareDocumentPosition(chrome) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(chrome.compareDocumentPosition(system) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(within(task(1)).queryByTestId("guide-note")).toBeNull();
    expect(within(task(1)).queryByText("other ways")).toBeNull();
    cleanup();

    draw({ os: "windows" });
    expect(document.body.textContent).not.toMatch(/Firefox/);
  });

  it("the one act leads task 1, and nothing renders between the tasks", () => {
    draw();

    expect(within(task(1)).getByRole("button", { name: "Set up this Mac" })).toBeInTheDocument();
    expect(task(1)).toHaveTextContent(
      "one password prompt — it installs the certificate and sets the proxy",
    );
    const between = [...screen.getByTestId("steps").children].filter(
      (child) => !/^task-\d$/.test(child.getAttribute("data-testid") ?? ""),
    );
    expect(between).toHaveLength(0);
  });
});

// ── Actions follow their task's reading (E-ONBREV.W4, R12–R18) ──────────────
// What the host can do decides which actions exist; what each probe reads
// decides which the screen offers, and where. Done is the probe's reading.

const readings = (trust: "trusted" | "absent", proxy: "ours" | "none"): Props => ({
  trustProbe: trust,
  proxy: { status: proxy, address: null },
});

const openEveryDisclosure = () =>
  document.querySelectorAll("details").forEach((d) => d.setAttribute("open", ""));

describe.each(["macos", "windows"] as const)("on %s, the layout is a reading", (os) => {
  const here = os === "macos" ? "this Mac" : "this PC";

  // P8, inverted: the owner's case.
  it("certificate trusted, proxy off: the proxy task carries its own action and steps", () => {
    draw({ os, ...readings("trusted", "none") });

    expect(screen.queryByRole("button", { name: `Set up ${here}` })).toBeNull();
    expect(screen.queryByText(/one password prompt/)).toBeNull();
    expect(within(task(2)).getByRole("button", { name: "Set the system proxy" })).toBeInTheDocument();
    expect(within(task(2)).getByText(MANUAL)).toBeInTheDocument();
    expect(within(task(1)).getByText("to remove it later")).toBeInTheDocument();
    expect(within(task(1)).queryByText(MANUAL)).toBeNull();
    openEveryDisclosure();
    expect(screen.queryByRole("button", { name: "the trust command" })).toBeNull();
    expect(screen.queryByText(/Save Trawler's certificate/)).toBeNull();
    expect(within(task(2)).getByRole("button", { name: "the proxy command" })).toBeInTheDocument();
  });

  it("proxy set, certificate not trusted: the certificate's steps are open in task 1", () => {
    draw({ os, ...readings("absent", "ours") });

    expect(screen.queryByRole("button", { name: `Set up ${here}` })).toBeNull();
    expect(screen.queryByRole("button", { name: "Set the system proxy" })).toBeNull();
    expect(open("Save Trawler's certificate as trawler-ca.pem")).toBe(true);
    expect(within(task(1)).getByText("2 steps")).toBeInTheDocument();
    expect(within(task(2)).getByRole("button", { name: "stop routing through Trawler" }))
      .toBeInTheDocument();
    expect(task(2)).toHaveTextContent("this turns the proxy off and nothing else.");
    expect(task(2)).not.toHaveTextContent(/certificate/);
    expect(open("to turn it off later")).toBe(false);
  });

  it("both done: nothing left to install, each task keeps its way back, shut", () => {
    draw({ os, ...readings("trusted", "ours") });

    expect(screen.queryByRole("button", { name: `Set up ${here}` })).toBeNull();
    expect(screen.queryByRole("button", { name: "Set the system proxy" })).toBeNull();
    expect(open("to remove it later")).toBe(false);
    expect(open("to turn it off later")).toBe(false);
    openEveryDisclosure();
    expect(screen.queryByRole("button", { name: "the trust command" })).toBeNull();
    expect(screen.queryByRole("button", { name: "the proxy command" })).toBeNull();
    expect(screen.queryByText(MANUAL)).toBeNull();
  });
});

describe("a failed action opens the route its note points at (R16)", () => {
  const failure = { refused: false, detail: "no network services to configure" };

  it("a failed set-up opens the certificate's steps under it", async () => {
    draw({ onSetUpThisMachine: vi.fn(() => Promise.reject(failure)) });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() =>
      expect(screen.getByText("setting up failed — the steps below do it by hand.")).toBeInTheDocument(),
    );
    expect(open("Save Trawler's certificate as trawler-ca.pem")).toBe(true);
    expect(open("Run — this sets the Wi-Fi service; change the name for Ethernet:")).toBe(false);
  });

  it("the button's line stays under the button, above what happened", async () => {
    draw({ onSetUpThisMachine: vi.fn(() => Promise.reject(failure)) });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    const note = await screen.findByText("setting up failed — the steps below do it by hand.");
    const line = screen.getByText(
      "one password prompt — it installs the certificate and sets the proxy",
    );
    expect(line.compareDocumentPosition(note) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("a failed proxy action opens the proxy's steps", async () => {
    draw({ ...readings("trusted", "none"), onSetProxy: vi.fn(() => Promise.reject(failure)) });

    fireEvent.click(screen.getByRole("button", { name: "Set the system proxy" }));

    await waitFor(() =>
      expect(screen.getByText("setting the proxy failed — the steps below do it by hand.")).toBeInTheDocument(),
    );
    expect(open("Run — this sets the Wi-Fi service; change the name for Ethernet:")).toBe(true);
  });

  it("a failed reversal opens the steps that turn the proxy off", async () => {
    draw({ ...readings("trusted", "ours"), onUnsetProxy: vi.fn(() => Promise.reject(failure)) });

    fireEvent.click(screen.getByRole("button", { name: "stop routing through Trawler" }));

    await waitFor(() =>
      expect(screen.getByText("couldn't turn the proxy off — the steps below do it by hand.")).toBeInTheDocument(),
    );
    expect(open("to turn it off later")).toBe(true);
    expect(open("to remove it later")).toBe(false);
  });

  // Review round 2, blocker: a failed reversal took the one open guide away from
  // the certificate task while its refusal still said "the steps below".
  it("a failure elsewhere leaves open the steps a refusal points at", async () => {
    draw({
      ...readings("absent", "ours"),
      traffic: arriving,
      onUnsetProxy: vi.fn(() => Promise.reject(failure)),
    });

    fireEvent.click(screen.getByRole("button", { name: "stop routing through Trawler" }));

    await waitFor(() =>
      expect(screen.getByText("couldn't turn the proxy off — the steps below do it by hand.")).toBeInTheDocument(),
    );
    expect(within(task(1)).getByTestId("traffic-remedy")).toHaveTextContent("The steps below install it.");
    expect(open("Save Trawler's certificate as trawler-ca.pem")).toBe(true);
    expect(open("to turn it off later")).toBe(true);
  });

  it("a dismissed prompt opens nothing — the button can still do the job", async () => {
    draw({ onSetUpThisMachine: vi.fn(() => Promise.reject({ refused: true, detail: "(-128)" })) });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() => expect(screen.getByText(/password prompt was cancelled/)).toBeInTheDocument());
    expect(open("Save Trawler's certificate as trawler-ca.pem")).toBe(false);
  });
});

describe("the reversal sits in the task it reverses", () => {
  it("on the Simulator, a proxy that is ours offers the way out and not the action again (R15)", () => {
    draw(readings("absent", "ours"));
    pick("iOS Simulator");

    expect(screen.queryByRole("button", { name: "Set the system proxy" })).toBeNull();
    expect(within(task(2)).getByRole("button", { name: "stop routing through Trawler" }))
      .toBeInTheDocument();
    expect(task(2)).toHaveTextContent("this turns the proxy off and nothing else.");
  });

  it("appears in task 2 the moment the action resolves, before the re-read", async () => {
    draw({ trustProbe: "unknown", proxy: { status: "unknown", address: null } });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() =>
      expect(within(task(2)).getByRole("button", { name: "stop routing through Trawler" }))
        .toBeInTheDocument(),
    );
  });

  it("comes back when the proxy is set again after it was taken off", async () => {
    const props: Props = readings("trusted", "ours");
    const { rerender } = draw(props);

    fireEvent.click(screen.getByRole("button", { name: "stop routing through Trawler" }));
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "stop routing through Trawler" })).toBeNull(),
    );
    rerender(screenAt({ ...props, proxy: { status: "none", address: null } }));
    fireEvent.click(screen.getByRole("button", { name: "Set the system proxy" }));
    rerender(screenAt(props));

    await waitFor(() =>
      expect(within(task(2)).getByRole("button", { name: "stop routing through Trawler" }))
        .toBeInTheDocument(),
    );
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
      trustProbe: "absent",
      proxy: { status: "none", address: null },
      onSetUpThisMachine: vi.fn(() => Promise.reject(cancelled)),
    });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() => expect(screen.getByText(/password prompt was cancelled/)).toBeInTheDocument());
    expect(task(1)).toHaveTextContent("not done — not installed");
    expect(task(2)).toHaveTextContent("not done — the system proxy is off");
  });
});

// ── An approval the OS could not show (-60007) ─────────────────────────────
// R7. The user answered the password prompt; the trust write's own approval
// never reached them, so the screen may not say they cancelled it.

describe("an approval macOS could not show", () => {
  const unshowable = {
    refused: false,
    unshowable: true,
    detail:
      "0:207: execution error: SecTrustSettingsSetTrustSettings: The authorization was denied since no user interaction was possible. (-60007)",
  };

  it("says what happened, never that the user cancelled, with the tool's text one click away", async () => {
    draw({
      trustProbe: "absent",
      proxy: { status: "none", address: null },
      onSetUpThisMachine: vi.fn(() => Promise.reject(unshowable)),
    });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() =>
      expect(screen.getByTestId("set-up-status")).toHaveTextContent(
        "macOS couldn't show the prompt to approve this, so it isn't done. The steps below do it by hand.",
      ),
    );
    expect(screen.queryByText(/cancelled/)).toBeNull();
    fireEvent.click(screen.getByText("details"));
    expect(disclosing(unshowable.detail)).toHaveAttribute("open");
  });

  it("opens the steps it points at, and leaves the task's state to the probe", async () => {
    draw({
      trustProbe: "absent",
      proxy: { status: "none", address: null },
      onSetUpThisMachine: vi.fn(() => Promise.reject(unshowable)),
    });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() => expect(screen.getByTestId("set-up-status")).toHaveAttribute("data-state", "failed"));
    expect(open("Save Trawler's certificate as trawler-ca.pem")).toBe(true);
    expect(task(1)).toHaveTextContent("not done — not installed");
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
  const notInIt = [{ name: "iPhone 17 Pro", trust: "absent" as const }];
  const simulator = () => {
    draw({ trustProbe: "trusted", proxy: { status: "none", address: null }, simulators: notInIt });
    pick("iOS Simulator");
  };

  it("takes its trust task's state from its own store, never this Mac's", () => {
    simulator();

    expect(task(1)).toHaveTextContent("Trust Trawler's certificate in the Simulator");
    expect(task(1)).toHaveTextContent("not done — not in iPhone 17 Pro");
    expect(task(1)).not.toHaveTextContent("done — installed and trusted");
  });

  it("takes its routing task's state from this Mac, which it follows", () => {
    simulator();

    expect(task(2)).toHaveTextContent("Send this Mac's traffic to Trawler");
    expect(task(2)).toHaveTextContent("not done — the system proxy is off");
    expect(within(task(2)).getByRole("button", { name: "Set the system proxy" })).toBeInTheDocument();
  });

  it("renders its trust guide open under a count — nothing automatic stands above it", () => {
    simulator();

    expect(within(task(1)).queryByText("do it manually")).toBeNull();
    expect(within(task(1)).getByText("3 steps")).toBeInTheDocument();
    expect(open("3 steps")).toBe(true);
    expect(commandNamed("the trust command")).toHaveTextContent(
      "for d in $(xcrun simctl list devices booted | grep -Eo '[0-9A-F-]{36}'); do xcrun simctl keychain $d add-root-cert ~/Downloads/trawler-ca.pem; done",
    );
  });

  it("leaves the routing steps behind the button that does them", () => {
    simulator();

    expect(within(task(2)).getByText("do it manually")).toBeInTheDocument();
    expect(open("do it manually")).toBe(false);
  });

  it("is told to open a website in it, not on it", () => {
    simulator();

    expect(task(3)).toHaveTextContent("Open any website in the Simulator");
  });
});

// ── The Simulator's wire is this Mac's too (W7 · S1–S3) ─────────────────────
// It sends through this Mac's proxy, so App hands it its marked request alone
// as its traffic, and the whole wire beside it.

describe("the iOS Simulator, beside this Mac's own traffic", () => {
  const MARK = "k3m9";
  const sim = (trust: "trusted" | "absent" | "unknown") => [{ name: "iPhone 17 Pro", trust }];
  const simulator = (props: Props) => {
    draw({ proxy: { status: "ours", address: ADDR }, deviceMark: MARK, ...props });
    pick("iOS Simulator");
  };
  const wire = (traffic: Traffic) => ({ traffic: NO_TRAFFIC, device: { provedAt: null, mixed: true, wire: traffic } });

  // S1: This machine set up first, its browser decrypting - the common path.
  it("is not proven by this Mac's decrypted traffic", () => {
    simulator({ simulators: [], ...wire(capturing) });

    expect(task(1)).toHaveTextContent("can't check here — the address in step 3 confirms it");
    expect(task(3)).toHaveTextContent("can't check here — the address below confirms it");
    expect(zone("proven")).toBeNull();
  });

  it("offers its marked address, to open in it", () => {
    simulator({ simulators: [], ...wire(NO_TRAFFIC) });

    expect(zone("check-address")).toHaveTextContent("or open this one in the Simulator");
    expect(zone("check-address")).toHaveTextContent(`https://example.com/?trawler-check=${MARK}`);
  });

  // S2: an app on this Mac that pins its certificate.
  it("is never blamed for this Mac's refusal", () => {
    simulator({ simulators: sim("absent"), ...wire(arriving) });

    expect(task(1)).toHaveTextContent("not done — not in iPhone 17 Pro");
    expect(task(1)).not.toHaveTextContent("not working");
    expect(within(task(1)).queryByTestId("traffic-remedy")).toBeNull();
    expect(screen.queryByText(/the Simulator does not trust/)).toBeNull();
    expect(task(3)).toHaveTextContent("not working — arriving but not decrypting");
    expect(task(3)).toHaveTextContent("the device it comes from does not trust Trawler's certificate yet");
  });

  it("says a refusal is someone else's where its store reads trusted", () => {
    simulator({ simulators: sim("trusted"), ...wire(arriving) });

    expect(task(1)).toHaveTextContent("done — trusted in iPhone 17 Pro");
    expect(task(3)).toHaveTextContent(
      "iPhone 17 Pro trusts Trawler's certificate, so it comes from another device, or from an app with a certificate store of its own, that does not trust it yet.",
    );
    expect(task(3)).not.toHaveTextContent("Finish the certificate step");
  });

  // S3: its marked request, opened in it, is its proof and its end.
  it("is proven by its marked request, named and timed, with an end that names it", () => {
    const at = ago(1_000);
    const marked: Traffic = { ...NO_TRAFFIC, lastDecryptedAt: at };
    simulator({ simulators: [], traffic: marked, device: { provedAt: at, mixed: true, wire: marked } });

    expect(task(1)).toHaveTextContent(`done — the Simulator's request decrypted at ${clock(at)}`);
    expect(task(3)).toHaveTextContent(`done — decrypted at ${clock(at)}`);
    expect(zone("proven")).toHaveTextContent("You're set up — Trawler is capturing traffic from the Simulator");
  });

  it("keeps that proof past this Mac's refusal after it, as a note", () => {
    const at = ago(RECENT_MS * 2);
    const marked: Traffic = { ...NO_TRAFFIC, lastDecryptedAt: at };
    simulator({ simulators: sim("trusted"), traffic: marked, device: { provedAt: at, mixed: true, wire: { ...marked, ...arriving } } });

    expect(task(3)).toHaveTextContent(`done — decrypted at ${clock(at)}`);
    expect(task(3)).not.toHaveTextContent("not working");
  });
});

// ── The emulator: its own store and proxy through adb (W8 · S1–S4) ─────────
// It reaches Trawler on this machine's loopback, so App hands it what another
// device is handed: its marked request, and unmarked traffic only from a span
// this machine's proxy did not point here.

describe("the Android emulator, beside this machine's own traffic", () => {
  const MARK = "k3m9";
  const emu = (trust: "trusted" | "absent" | "unknown", proxy: EmulatorReading["proxy"] = "none") => [
    { name: "Pixel 7 API 35", trust, proxy },
  ];
  const emulator = (props: Props) => {
    draw({ proxy: { status: "ours", address: ADDR }, deviceMark: MARK, ...props });
    pick("Android emulator");
  };
  const mixedWire = (traffic: Traffic) => ({ traffic: NO_TRAFFIC, device: { provedAt: null, mixed: true, wire: traffic } });
  const clearWire = (traffic: Traffic) => ({ traffic, device: { provedAt: null, mixed: false, wire: NO_TRAFFIC } });

  // S1: This machine set up first, its browser decrypting - the common path.
  it("is not proven by this machine's decrypted traffic", () => {
    emulator({ emulators: [], ...mixedWire(capturing) });

    expect(task(1)).toHaveTextContent("can't check here — the address in step 3 confirms it");
    expect(task(2)).toHaveTextContent("can't check here — the same request confirms it");
    expect(task(3)).toHaveTextContent("can't check here — the address below confirms it");
    expect(zone("proven")).toBeNull();
    expect(zone("check-address")).toHaveTextContent("or open this one in the emulator");
    expect(zone("check-address")).toHaveTextContent(`https://example.com/?trawler-check=${MARK}`);
  });

  it("reads its own store and proxy, by the name of its AVD", () => {
    emulator({ emulators: emu("trusted", "ours"), ...mixedWire(NO_TRAFFIC) });

    expect(task(1)).toHaveTextContent("done — trusted in Pixel 7 API 35");
    expect(task(2)).toHaveTextContent("done — Pixel 7 API 35 routes through Trawler");
    expect(zone("probe")).toBeInTheDocument();
  });

  // S2: its own services refuse a certificate it holds, and never stop.
  it("never blames its certificate for a refusal", () => {
    emulator({ emulators: emu("unknown"), ...clearWire(arriving) });

    expect(task(1)).toHaveTextContent("can't check here — the emulator's first HTTPS request confirms it");
    expect(task(1)).not.toHaveTextContent("not working");
    expect(within(task(1)).queryByTestId("traffic-remedy")).toBeNull();
    expect(screen.queryByText(/the emulator does not trust|Finish the certificate step/)).toBeNull();
    expect(task(3)).toHaveTextContent("not working — arriving but not decrypting");
    expect(task(3)).toHaveTextContent("Android's own services ignore a user certificate, so they never decrypt");
  });

  it("names the store that trusts it when a refusal comes", () => {
    emulator({ emulators: emu("trusted", "ours"), ...clearWire(arriving) });

    expect(task(3)).toHaveTextContent(
      "Pixel 7 API 35 trusts Trawler's certificate, so it is Android's own services, which ignore a user certificate, or an app that pins its certificate.",
    );
  });

  // S4 · C4: the Wi-Fi proxy set, the emulator sending over mobile data.
  it("says why a Wi-Fi proxy routes nothing", () => {
    emulator({ emulators: emu("trusted", "wifi-only"), ...clearWire(capturing) });

    expect(task(2)).toHaveTextContent("not done — Pixel 7 API 35 sends over mobile data, not Wi-Fi");
  });

  // S3: its marked request is its proof and its end, kept past its services' refusals.
  it("is proven by its marked request, named, timed and kept, with an end that names it", () => {
    const at = ago(1_000);
    const marked: Traffic = { ...NO_TRAFFIC, lastDecryptedAt: at };
    emulator({ emulators: [], traffic: marked, device: { provedAt: at, mixed: true, wire: marked } });

    expect(task(1)).toHaveTextContent(`done — the emulator's request decrypted at ${clock(at)}`);
    expect(task(2)).toHaveTextContent(`done — the emulator's request came through at ${clock(at)}`);
    expect(zone("proven")).toHaveTextContent("You're set up — Trawler is capturing traffic from the emulator");
    cleanup();

    const earlier = ago(RECENT_MS * 2);
    const kept: Traffic = { ...NO_TRAFFIC, lastDecryptedAt: earlier };
    emulator({ emulators: [], traffic: kept, device: { provedAt: earlier, mixed: true, wire: { ...kept, ...arriving } } });
    expect(task(3)).toHaveTextContent(`done — decrypted at ${clock(earlier)}`);
    expect(task(3)).toHaveTextContent("Android's own services, which ignore a user certificate");
    expect(task(3)).not.toHaveTextContent("not working");
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
      "other devices on Wi-Fi will be able to use this proxy and see captured traffic until Trawler quits",
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
      screen.getByRole("button", { name: "Allow devices on Wi-Fi to connect" }),
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

    expect(task(1)).toHaveTextContent("done — listening on 192.168.1.24 until Trawler quits");
    expect(within(task(1)).getByRole("button", { name: "stop listening" })).toBeInTheDocument();
  });

  it("renders the certificate route the phone opens, and the address it points at", () => {
    armedPhone();

    expect(
      screen.getByRole("button", { name: /copy the certificate URL/i }),
    ).toHaveTextContent("http://192.168.1.24:7421/ca.mobileconfig");
  });

  it("saves the profile the iPhone installs, and names it by the file it is", async () => {
    const onSaveCa = vi.fn(async () => "/Users/me/Downloads/trawler-ca.mobileconfig");
    draw({ reach: ARMED, onSaveCa });
    pick("iPhone or iPad");

    fireEvent.click(within(task(2)).getByRole("button", { name: "save…" }));

    expect(onSaveCa).toHaveBeenCalledWith("mobileconfig");
    expect(
      await within(task(2)).findByText("saved as trawler-ca.mobileconfig. now trust it with the steps here."),
    ).toBeInTheDocument();
    expect(within(zone("device-address")!).getByRole("button", { name: /copy the proxy host/i }))
      .toHaveTextContent("192.168.1.24");
    expect(within(zone("device-address")!).getByRole("button", { name: /copy the proxy port/i }))
      .toHaveTextContent("9999");
  });

  it("promises to confirm on the phone's own first request, and claims nothing now", () => {
    armedPhone();

    expect(task(2)).toHaveTextContent("can't check here — your iPhone's first HTTPS request confirms it");
    expect(task(3)).toHaveTextContent("can't check here — your iPhone's first request confirms it");
    expect(task(2)).not.toHaveAttribute("data-dim");
  });

  it("names no automatic route it does not have — a hand is the only one it has", () => {
    armedPhone();

    expect(screen.queryByText("do it manually")).toBeNull();
    expect(task(2)).toHaveTextContent(/Settings → General → VPN & Device Management/);
  });

  it("opens the first task whose work is not done, and holds the next behind its count", () => {
    armedPhone();

    expect(within(task(2)).getByText("3 steps")).toBeInTheDocument();
    expect(open("3 steps")).toBe(true);
    expect(within(task(3)).getByText("2 steps")).toBeInTheDocument();
    expect(open("2 steps")).toBe(false);
  });

  it("closes every guide once a decrypted request says the job is over", () => {
    draw({ reach: ARMED, traffic: capturing });
    pick("iPhone or iPad");

    expect(document.querySelectorAll("details[open]")).toHaveLength(0);
    expect(task(2)).toHaveTextContent(`done — a request decrypted at ${clock(capturing.lastDecryptedAt!)}`);
    expect(task(3)).toHaveTextContent(`done — a request reached Trawler at ${clock(capturing.lastDecryptedAt!)}`);
  });
});

// W9: Apple's own services pin their certificates and refuse from the moment an
// iPhone's proxy points here, so its test is the address its try task offers.
describe("the iPhone, whose test is the address it is offered", () => {
  const TRUST_SWITCH = "Settings → General → About → Certificate Trust Settings → Trawler Proxy CA";
  const refusedAt = arriving.openTunnelAt!;
  const theAddress = (at: number, mine = true) => ({ host: "example.com", at, mine });
  const clearSpan = (refusal?: ReturnType<typeof theAddress>) => ({
    provedAt: null,
    mixed: false,
    wire: NO_TRAFFIC,
    refusal,
  });

  // DEC-63 as W4 left it, on a platform whose own services do not refuse.
  it("another computer's refusal still confirms its proxy and points at its certificate", () => {
    draw({ reach: ARMED, traffic: arriving });
    pick("Another Mac");

    expect(task(3)).toHaveTextContent(`done — a request reached Trawler at ${clock(refusedAt)}`);
    expect(within(task(3)).getByText("to turn it off later")).toBeInTheDocument();
    expect(task(2)).toHaveTextContent("not working — nothing decrypted");
    expect(within(task(2)).getByTestId("traffic-remedy")).toHaveTextContent(
      /^something reached Trawler and did not decrypt — that Mac does not trust Trawler's certificate yet\. The steps below install it\./,
    );
    expect(task(4)).toHaveTextContent("not working — arriving but not decrypting");
    expect(within(task(4)).queryByTestId("traffic-remedy")).toBeNull();
  });

  it("a refusal of anything else confirms the proxy, leaves the certificate to the address, and names Apple's services", () => {
    draw({ reach: ARMED, traffic: arriving, device: clearSpan() });
    pick("iPhone or iPad");

    expect(task(3)).toHaveTextContent(`done — a request reached Trawler at ${clock(refusedAt)}`);
    expect(task(2)).toHaveTextContent("can't check here — the address in step 4 confirms it");
    expect(within(task(2)).queryByTestId("traffic-remedy")).toBeNull();
    expect(task(4)).toHaveTextContent("not working — arriving but not decrypting");
    expect(within(task(4)).getByTestId("traffic-remedy")).toHaveTextContent(
      "something is connecting and none of it is decrypting — Apple's own services pin their certificates, so they never decrypt. The address below is the test.",
    );
  });

  it("its address refused says the trust switch before the profile, with the steps open, once", () => {
    draw({ reach: ARMED, traffic: arriving, device: clearSpan(theAddress(refusedAt)) });
    pick("iPhone or iPad");

    expect(task(2)).toHaveTextContent("not working — example.com did not decrypt");
    expect(within(task(2)).getByTestId("traffic-remedy")).toHaveTextContent(
      `example.com did not decrypt on your iPhone — turn on full trust: ${TRUST_SWITCH}. Not listed there? The profile is not installed — the steps below install it.`,
    );
    expect(open("On your iPhone, open")).toBe(true);
    expect(task(4)).toHaveTextContent("not working — arriving but not decrypting");
    expect(within(task(4)).queryByTestId("traffic-remedy")).toBeNull();
  });

  // The review's round 1: a retry may yet decrypt, so until it does or is
  // refused itself, the refusal before it stands.
  it("its address retried inside the retry's grace keeps the refusal before it", () => {
    const { rerender } = draw({ reach: ARMED, traffic: arriving, device: clearSpan(theAddress(refusedAt)) });
    pick("iPhone or iPad");
    expect(task(2)).toHaveTextContent("not working — example.com did not decrypt");

    const retriedAt = Date.now();
    const retrying = { ...arriving, lastTunnelAt: retriedAt };
    rerender(screenAt({ reach: ARMED, traffic: retrying, device: clearSpan(theAddress(retriedAt)) }));

    expect(task(2)).toHaveTextContent("not working — example.com did not decrypt");
    expect(within(task(2)).getByTestId("traffic-remedy")).toHaveTextContent(/turn on full trust/);
  });

  it("its address is not refused inside a tunnel's grace", () => {
    const opening = { ...NO_TRAFFIC, lastTunnelAt: ago(500), openTunnelAt: ago(500) };
    draw({ reach: ARMED, traffic: opening, device: clearSpan(theAddress(opening.openTunnelAt)) });
    pick("iPhone or iPad");

    expect(task(2)).not.toHaveTextContent("not working");
  });

  // Q2: a proof older than the refusal showed a certificate since removed, or
  // a trust switch since turned off.
  it("its address refused after a proof reopens the certificate task; the try task keeps the proof and the address to check again", () => {
    const provedAt = ago(60_000);
    const traffic = { ...arriving, lastDecryptedAt: provedAt };
    draw({ reach: ARMED, traffic, device: clearSpan(theAddress(refusedAt)), deviceMark: "k3m9" });
    pick("iPhone or iPad");

    expect(task(2)).toHaveTextContent(
      `not working — example.com did not decrypt at ${clock(refusedAt)}, after decrypting at ${clock(provedAt)}`,
    );
    expect(within(task(2)).getByTestId("traffic-remedy")).toHaveTextContent(/turn on full trust/);
    expect(open("On your iPhone, open")).toBe(true);
    expect(task(4)).toHaveTextContent(`done — decrypted at ${clock(provedAt)}`);
    expect(within(task(4)).queryByTestId("traffic-remedy")).toBeNull();
    expect(screen.getByTestId("check-address")).toHaveTextContent("to check again, open this one on your iPhone");
  });

  it("its address refused before a proof leaves the proof standing", () => {
    const provedAt = ago(60_000);
    draw({
      reach: ARMED,
      traffic: { ...NO_TRAFFIC, lastDecryptedAt: provedAt },
      device: clearSpan(theAddress(ago(90_000))),
    });
    pick("iPhone or iPad");

    expect(task(2)).toHaveTextContent(`done — a request decrypted at ${clock(provedAt)}`);
  });

  it("after a proof, a refusal of anything else is Apple's own services first", () => {
    const provedAt = ago(60_000);
    draw({ reach: ARMED, traffic: { ...arriving, lastDecryptedAt: provedAt }, device: clearSpan() });
    pick("iPhone or iPad");

    expect(task(2)).toHaveTextContent(`done — a request decrypted at ${clock(provedAt)}`);
    expect(within(task(4)).getByTestId("traffic-remedy")).toHaveTextContent(
      "something else is connecting and not decrypting — one request already did, so it is Apple's own services, which pin their certificates, an app that pins its certificate, or another device.",
    );
  });

  // Rule 3: while this Mac's traffic may be arriving, nothing unmarked is the
  // iPhone's, its address refused included.
  it("its address refused while this Mac's traffic may be arriving is nobody's, and the try task says what to do if it was the iPhone's", () => {
    draw({
      reach: ARMED,
      device: { provedAt: null, mixed: true, wire: arriving, refusal: theAddress(refusedAt, false) },
      deviceMark: "k3m9",
    });
    pick("iPhone or iPad");

    expect(task(2)).toHaveTextContent("can't check here — the address in step 4 confirms it");
    expect(within(task(2)).queryByTestId("traffic-remedy")).toBeNull();
    expect(within(task(4)).getByTestId("traffic-remedy")).toHaveTextContent(
      `example.com did not decrypt — if you opened it on your iPhone, turn on full trust: ${TRUST_SWITCH}. Not listed there? The profile is not installed — the certificate step installs it.`,
    );
    expect(document.body.textContent).not.toMatch(/Finish the certificate step/);
  });

  it("anything else refused while this Mac's traffic may be arriving names Apple's services, never the certificate step", () => {
    draw({ reach: ARMED, device: { provedAt: null, mixed: true, wire: arriving }, deviceMark: "k3m9" });
    pick("iPhone or iPad");

    expect(within(task(4)).getByTestId("traffic-remedy")).toHaveTextContent(/Apple's own services pin their certificates/);
    expect(document.body.textContent).not.toMatch(/Finish the certificate step/);
  });
});

// A1 · A3 · A4 (DEC-63): with this Mac routed through Trawler, its own
// requests arrive beside that Mac's, and nothing unmarked tells them apart.
describe("another device, where this machine's own traffic may be arriving too", () => {
  const MARK = "k3m9";
  const mixedIn = (wire: Traffic, provedAt: number | null = null) => ({ provedAt, mixed: true, wire });

  it("nothing arriving settles a task, and each names what will", () => {
    draw({ reach: ARMED, device: mixedIn(capturing), deviceMark: MARK });
    pick("Another Mac");

    expect(task(2)).toHaveTextContent("can't check here — the address in step 4 confirms it");
    expect(task(3)).toHaveTextContent("can't check here — the same request confirms it");
    expect(task(4)).toHaveTextContent("can't check here — the address below confirms it");
    expect(zone("proven")).toBeNull();
  });

  it("offers that address in the try task, carrying its mark", () => {
    draw({ reach: ARMED, deviceMark: MARK });
    pick("Another Mac");

    const address = within(task(4)).getByTestId("check-address");
    expect(address).toHaveTextContent("or open this one on that Mac");
    expect(within(address).getByRole("button", { name: /copy the address to open/i })).toHaveTextContent(
      `https://example.com/?trawler-check=${MARK}`,
    );
    expect(within(address).queryByTestId("scan-code")).toBeNull();
  });

  it("gives a phone the address as a code to scan as well (DEC-57)", () => {
    draw({ reach: ARMED, deviceMark: MARK });
    pick("iPhone or iPad");

    expect(within(task(4)).getByTestId("check-address")).toHaveTextContent("or open this one on your iPhone");
    expect(within(task(4)).getByTestId("scan-code")).toBeInTheDocument();
  });

  it.each([
    ["may be arriving", true],
    ["cannot be arriving", false],
  ])(
    "its marked request settles every task, named and timed, and the end names that Mac, where this Mac's own traffic %s",
    (_how, mixed) => {
      const at = Date.now() - 1_000;
      const marked = { ...NO_TRAFFIC, lastDecryptedAt: at };
      draw({ reach: ARMED, traffic: marked, device: { provedAt: at, mixed, wire: capturing }, deviceMark: MARK });
      pick("Another Mac");

      expect(task(2)).toHaveTextContent(`done — that Mac's request decrypted at ${clock(at)}`);
      expect(task(3)).toHaveTextContent(`done — that Mac's request came through at ${clock(at)}`);
      expect(task(4)).toHaveTextContent(mixed ? `done — decrypted at ${clock(at)}` : "done — capturing traffic");
      expect(zone("proven")).toHaveTextContent("You're set up — Trawler is capturing traffic from that Mac");
    },
  );

  // Rule 4: what arrived while this Mac's own could not was that Mac's then.
  it("a proof already that Mac's stays, with its time", () => {
    const at = ago(40_000);
    draw({ reach: ARMED, traffic: { ...NO_TRAFFIC, lastDecryptedAt: at }, device: mixedIn(capturing), deviceMark: MARK });
    pick("Another Mac");

    expect(task(2)).toHaveTextContent(`done — a request decrypted at ${clock(at)}`);
    expect(task(3)).toHaveTextContent(`done — a request reached Trawler at ${clock(at)}`);
    expect(task(4)).toHaveTextContent(`done — decrypted at ${clock(at)}`);
  });

  it("a refusal is not that Mac's to own: the try task says it and names no device (A3)", () => {
    draw({ reach: ARMED, device: mixedIn(arriving), deviceMark: MARK });
    pick("Another Mac");

    expect(task(2)).toHaveTextContent("can't check here — the address in step 4 confirms it");
    expect(within(task(2)).queryByTestId("traffic-remedy")).toBeNull();
    expect(task(4)).toHaveTextContent("not working — arriving but not decrypting");
    expect(within(task(4)).getByTestId("traffic-remedy")).toHaveTextContent(
      "the device it comes from does not trust Trawler's certificate yet",
    );
    expect(document.body.textContent).not.toMatch(/that Mac does not trust/);
  });

  it("a proof already that Mac's outlasts the refusal, which is a note under it (S1)", () => {
    const at = ago(40_000);
    draw({ reach: ARMED, traffic: { ...NO_TRAFFIC, lastDecryptedAt: at }, device: mixedIn(arriving, at), deviceMark: MARK });
    pick("Another Mac");

    expect(task(4)).toHaveTextContent(`done — decrypted at ${clock(at)}`);
    expect(within(task(4)).getByTestId("traffic-remedy")).toHaveTextContent("one request already did");
    expect(document.body.textContent).not.toMatch(/Finish the certificate step/);
  });

  it("where it cannot be arriving, unmarked traffic is that Mac's, and the end names no one", () => {
    draw({ reach: ARMED, traffic: capturing, deviceMark: MARK });
    pick("Another Mac");

    expect(task(2)).toHaveTextContent("done — a request decrypted at");
    expect(task(3)).toHaveTextContent("done — a request reached Trawler at");
    expect(zone("proven")).toHaveTextContent("You're set up — Trawler is capturing traffic");
    expect(zone("proven")).not.toHaveTextContent(/that Mac/);
  });

  it("names that Mac at the end only while its marked request is the newest proof", () => {
    draw({
      reach: ARMED,
      traffic: capturing,
      device: { provedAt: ago(20_000), mixed: false, wire: capturing },
      deviceMark: MARK,
    });
    pick("Another Mac");

    expect(task(2)).toHaveTextContent("done — that Mac's request decrypted at");
    expect(zone("proven")).not.toHaveTextContent(/that Mac/);
  });
});

// R1: a network change leaves the listener on an address this Mac no longer
// holds, and every address the tasks below print with it.
describe("an allowed address this machine has left", () => {
  const left: ReachState = { ...ARMED, interfaces: [{ name: "Wi-Fi", address: "10.0.4.9" }] };

  it("is not working, and named", () => {
    draw({ reach: left });
    pick("Another Mac");

    expect(task(1)).toHaveTextContent("not working — this Mac no longer holds 192.168.1.24");
    expect(task(1)).not.toHaveTextContent(/listening on/);
  });

  it("offers the address this machine holds now", async () => {
    const onArmReach = vi.fn(async () => {});
    draw({ reach: left, onArmReach });
    pick("Another Mac");

    expect(zone("listen-on")).toHaveTextContent("Wi-Fi · 10.0.4.9");
    fireEvent.click(screen.getByRole("button", { name: "Allow devices on Wi-Fi to connect" }));
    await waitFor(() => expect(onArmReach).toHaveBeenCalledWith("10.0.4.9"));
  });

  it("dims what depends on it, so no step prints an address nothing answers at", () => {
    draw({ reach: left, traffic: capturing });
    pick("Another Mac");

    for (const n of [2, 3, 4]) expect(task(n)).toHaveAttribute("data-dim");
    expect(zone("device-address")).toBeNull();
    expect(document.body.textContent).not.toMatch(/192\.168\.1\.24:7421/);
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

    fireEvent.click(screen.getByRole("button", { name: "Allow devices on Wi-Fi to connect" }));

    await waitFor(() => expect(onArmReach).toHaveBeenCalledWith("192.168.1.24"));
  });

  it("a refused confirm says what is still true", async () => {
    draw({ reach: UNARMED, onArmReach: vi.fn(async () => null) });
    pick("iPhone or iPad");

    fireEvent.click(screen.getByRole("button", { name: "Allow devices on Wi-Fi to connect" }));

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

  it("names the one address it will arm before anything is armed (R9)", () => {
    draw({ reach: { ...UNARMED, interfaces: [{ name: "Ethernet", address: "192.168.1.23" }] } });
    pick("iPhone or iPad");

    expect(zone("listen-on")).toHaveTextContent("Ethernet · 192.168.1.23");
    expect(screen.queryByText("other addresses")).toBeNull();
  });

  it("names the interface it arms, in its button and its warning (R21)", () => {
    draw({ reach: { ...UNARMED, interfaces: [{ name: "Ethernet", address: "192.168.1.23" }] } });
    pick("iPhone or iPad");

    expect(
      screen.getByRole("button", { name: "Allow devices on Ethernet to connect" }),
    ).toBeInTheDocument();
    expect(task(1)).toHaveTextContent("other devices on Ethernet will be able to use this proxy");
    expect(task(1)).not.toHaveTextContent(/Wi-Fi/);
  });

  it("names the interface of the address taken instead (R21)", () => {
    draw({ reach: UNARMED });
    pick("iPhone or iPad");

    fireEvent.click(screen.getByText("other addresses"));
    fireEvent.click(screen.getByRole("button", { name: "Ethernet · 10.0.4.7" }));

    expect(
      screen.getByRole("button", { name: "Allow devices on Ethernet to connect" }),
    ).toBeInTheDocument();
  });

  it("with no address, says how to get one and re-reads on demand (R22)", () => {
    const onRereadReach = vi.fn();
    draw({ reach: { ...UNARMED, interfaces: [] }, onRereadReach });
    pick("iPhone or iPad");

    expect(task(1)).toHaveTextContent("Join the network your iPhone is on, then check again.");
    fireEvent.click(within(task(1)).getByRole("button", { name: "check again" }));
    expect(onRereadReach).toHaveBeenCalledOnce();
  });

  it("with no address, the tasks below wait on nothing that cannot be done (R22)", () => {
    draw({ reach: { ...UNARMED, interfaces: [] } });
    pick("iPhone or iPad");

    for (const n of [2, 3, 4]) {
      expect(task(n)).toHaveAttribute("data-dim");
      expect(task(n)).not.toHaveTextContent("after step 1");
    }
  });
});

// ── The end of setup ────────────────────────────────────────────────────────
// F22's other half: the moment it succeeds is said out loud, here and on the
// list the user lands on.

describe("the end", () => {
  it("says setup worked, and names the device it worked for", () => {
    draw({ traffic: capturing, ...readings("trusted", "ours") });

    expect(zone("proven")).toHaveTextContent(
      "You're set up — Trawler is capturing traffic from this Mac",
    );
  });

  // R17: the probes beside it say the request cannot have come from this Mac.
  it.each([
    ["absent", "none"],
    ["trusted", "none"],
    ["absent", "ours"],
  ] as const)("names no device where this Mac reads certificate %s and proxy %s", (trust, proxy) => {
    draw({ traffic: capturing, ...readings(trust, proxy) });

    expect(zone("proven")).toHaveTextContent("You're set up — Trawler is capturing traffic");
    expect(zone("proven")).not.toHaveTextContent(/this Mac/);
  });

  // R18: the remedy may not send the user back to a step task 1 reads as done.
  it("an arriving connection beside a trusted certificate names somewhere else to look", () => {
    draw({ traffic: arriving, ...readings("trusted", "ours") });

    expect(zone("traffic-remedy")).toHaveTextContent(
      "something is connecting and none of it is decrypting — this Mac trusts Trawler's certificate, so it comes from another device, or from an app with a certificate store of its own, that does not trust it yet.",
    );
    expect(zone("traffic-remedy")).not.toHaveTextContent(/Finish the certificate step/);
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

  it("an arriving connection that will not decrypt is said in the certificate task, with its steps open", () => {
    draw({ traffic: arriving, trustProbe: "absent" });

    expect(within(task(1)).getByTestId("traffic-remedy")).toHaveTextContent(
      "something reached Trawler and did not decrypt — this Mac does not trust Trawler's certificate yet. The steps below install it. Already did? A browser reads the system's store when it starts — restart it. Apps that pin their own certificate never decrypt.",
    );
    expect(open("Save Trawler's certificate as trawler-ca.pem")).toBe(true);
    expect(within(task(3)).queryByTestId("traffic-remedy")).toBeNull();
  });

  // Q3 · DEC-61: the list is a screen to switch to, not a reward for finishing.
  it("the way back is there whether or not capture has been proven", () => {
    for (const everProven of [false, true]) {
      const dispatch = vi.fn();
      const { unmount } = draw({ state: { ...onboarding, everProven }, dispatch });

      fireEvent.click(screen.getByRole("button", { name: COPY.back }));
      expect(dispatch).toHaveBeenCalledWith({ t: "exit" });
      unmount();
    }
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

  it.each(["macos", "windows"] as const)("is offered for this machine on %s, which can read it", (os) => {
    draw({ os, probedAt: ago(60_000) });

    expect(zone("probe")).toBeInTheDocument();
  });

  it("is absent for this machine on a host with no reader, where no probe can answer (R20)", () => {
    draw({ os: "other", trustProbe: "unknown", proxy: { status: "unknown", address: null } });

    expect(zone("probe")).toBeNull();
    expect(screen.queryByRole("button", { name: "check again" })).toBeNull();
  });

  // D8: Linux reads its CA bundle and GNOME's proxy, so it can re-read them.
  it("is offered on Linux, which reads its CA bundle and GNOME's proxy (D8)", () => {
    draw({ os: "linux", trustProbe: "absent", proxy: { status: "none", address: null } });

    expect(zone("probe")).toBeInTheDocument();
    expect(task(1)).toHaveTextContent("not done — not installed");
    expect(task(2)).toHaveTextContent("not done — the system proxy is off");
  });

  it("on Linux, a proxy that could not be read is a promise, never \"off\" (R19)", () => {
    draw({ os: "linux", proxy: { status: "unknown", address: null } });

    expect(task(2)).toHaveTextContent("can't check here — the same request confirms it");
    expect(task(2)).not.toHaveTextContent("the system proxy is off");
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
    const at = Date.now();
    draw({ traffic: { ...NO_TRAFFIC, lastDecryptedAt: at } });
    expect(zone("proven")).toBeInTheDocument();

    resolved.mockClear();
    act(() => vi.advanceTimersByTime(RECENT_MS + 2_000));

    expect(zone("proven")).toBeNull();
    expect(task(3)).toHaveTextContent(`done — decrypted at ${clock(at)}, nothing since`);
    expect(resolved).not.toHaveBeenCalled();
  });

  // U5 · Q3 · P1: 30 s after "You're set up" the try task fell back to "not
  // done" under two done tasks. On This machine and on another device the
  // proof stays, with its time; their other two tasks keep theirs. The
  // emulator is another device (W8's S3).
  it("keeps the proof on This machine and on another device once the traffic goes quiet", () => {
    const at = Date.now();
    draw({ reach: ARMED, traffic: { ...NO_TRAFFIC, lastDecryptedAt: at } });
    act(() => vi.advanceTimersByTime(RECENT_MS + 2_000));
    expect(task(3)).toHaveTextContent(`done — decrypted at ${clock(at)}, nothing since`);

    pick("iPhone or iPad");
    expect(task(4)).toHaveTextContent(`done — decrypted at ${clock(at)}, nothing since`);
    expect(task(2)).toHaveTextContent(`done — a request decrypted at ${clock(at)}`);

    pick("Android emulator");
    expect(task(3)).toHaveTextContent(`done — decrypted at ${clock(at)}, nothing since`);
    expect(task(1)).toHaveTextContent(`done — a request decrypted at ${clock(at)}`);
  });

  // S1: Windows starts its own HTTPS the moment its proxy is set, and an app of
  // it that pins its certificate refused - turning step 4 red under a done
  // certificate step, and telling the user to redo it.
  it("keeps another device's proof past a refusal and past plain http, the refusal a note", () => {
    const at = ago(40_000);
    const clear = { provedAt: null, mixed: false, wire: NO_TRAFFIC };
    const refused = { ...NO_TRAFFIC, lastDecryptedAt: at, lastTunnelAt: ago(5_000), openTunnelAt: ago(5_000) };
    const { rerender } = draw({ reach: ARMED, traffic: refused, device: clear, deviceMark: "k3m9" });
    pick("Another Windows PC");

    expect(task(2)).toHaveTextContent(`done — a request decrypted at ${clock(at)}`);
    expect(task(4)).toHaveTextContent(`done — decrypted at ${clock(at)}`);
    expect(within(task(4)).getByTestId("traffic-remedy")).toHaveTextContent(
      "something else is connecting and not decrypting — one request already did",
    );
    expect(document.body.textContent).not.toMatch(/not working|Finish the certificate step/);

    rerender(screenAt({ reach: ARMED, traffic: { ...NO_TRAFFIC, lastDecryptedAt: at, lastPlainAt: ago(5_000) }, device: clear, deviceMark: "k3m9" }));
    expect(task(4)).toHaveTextContent(`done — decrypted at ${clock(at)}`);
    expect(document.body.textContent).not.toMatch(/only plain http/);
  });
});

// ── W6: another Ubuntu machine's browsers keep stores of their own ─────────

describe("another Ubuntu machine", () => {
  const clear = { provedAt: null, mixed: false, wire: NO_TRAFFIC };

  // C2 · C3 · C4: certutil is not on a stock Ubuntu, Chrome's database moved at
  // M146, and the download folder is named in the session's language.
  it("prints commands that run on a stock Ubuntu, on the file its download step left", () => {
    draw({ reach: ARMED, device: clear, deviceMark: "k3m9" });
    pick("Another Ubuntu machine");

    expect(commandNamed("the Chrome command")).toHaveTextContent(
      'sudo apt install -y libnss3-tools && { [ -d ~/.pki/nssdb ] || [ -d ~/.local/share/pki/nssdb ] || { mkdir -p ~/.local/share/pki/nssdb && certutil -d sql:$HOME/.local/share/pki/nssdb -N --empty-password; }; } && certutil -d "sql:$([ -d ~/.pki/nssdb ] && echo ~/.pki/nssdb || echo ~/.local/share/pki/nssdb)" -A -t C,, -n trawler -i "$(xdg-user-dir DOWNLOAD)/trawler-ca.pem"',
    );
    expect(commandNamed("the trust command")).toHaveTextContent('sudo cp "$(xdg-user-dir DOWNLOAD)/trawler-ca.pem" ');
  });

  // S1: the system command reaches no browser, so a restart fixes nothing.
  it("says the system command reaches no browser while nothing decrypts", () => {
    draw({ reach: ARMED, traffic: arriving, device: clear, deviceMark: "k3m9" });
    pick("Another Ubuntu machine");

    expect(task(2)).toHaveTextContent("not working — nothing decrypted");
    const remedy = within(task(2)).getByTestId("traffic-remedy");
    expect(remedy).toHaveTextContent(
      "Already did? The system command covers curl and the system's tools, not browsers: Firefox and Chrome each keep a store of their own, and each has its own step below.",
    );
    expect(remedy.textContent).not.toMatch(/restart/);
  });

  // S2: a GNOME app reading the system store, or Chrome after its import, can
  // make the proof while Firefox still refuses.
  it("after a proof, names a browser with a store of its own first, mixed or not", () => {
    const at = ago(40_000);
    const refused = { ...NO_TRAFFIC, lastDecryptedAt: at, lastTunnelAt: ago(5_000), openTunnelAt: ago(5_000) };
    const BROWSER_FIRST =
      "one request already did, so it is a browser with a store of its own (Firefox, Chrome), an app that pins its certificate, or another device.";
    const { rerender } = draw({ reach: ARMED, traffic: refused, device: clear, deviceMark: "k3m9" });
    pick("Another Ubuntu machine");

    expect(task(4)).toHaveTextContent(`done — decrypted at ${clock(at)}`);
    expect(within(task(4)).getByTestId("traffic-remedy")).toHaveTextContent(BROWSER_FIRST);

    const mixed = { provedAt: at, mixed: true, wire: { ...NO_TRAFFIC, lastTunnelAt: ago(5_000), openTunnelAt: ago(5_000) } };
    rerender(screenAt({ reach: ARMED, traffic: refused, device: mixed, deviceMark: "k3m9" }));
    expect(within(task(4)).getByTestId("traffic-remedy")).toHaveTextContent(BROWSER_FIRST);
  });
});

// ── U4: a read nobody asked for is quiet ────────────────────────────────────

describe("a reading that lands without being asked for", () => {
  it("changes the lines it changes, and never shows them checking or holds a button", () => {
    const { rerender } = draw({ trustProbe: "absent", proxy: { status: "none", address: null } });
    expect(screen.getByRole("button", { name: "Set up this Mac" })).toBeEnabled();

    rerender(screenAt({ trustProbe: "absent", proxy: { status: "ours", address: "127.0.0.1:9999" } }));

    expect(task(1)).toHaveTextContent("not done — not installed");
    expect(task(2)).toHaveTextContent("done — this Mac routes through Trawler");
    expect(document.body.textContent).not.toMatch(/checking/);
  });
});

test("no step reports itself done — every state on the screen has a source", () => {
  draw({ trustProbe: "unknown", proxy: { status: "unknown", address: null }, os: "linux" });

  // Nothing read either store on this host, so neither task may claim one was.
  expect(task(1)).toHaveTextContent("can't check here — the request in step 3 confirms it");
  expect(task(2)).toHaveTextContent("can't check here — the same request confirms it");
  expect(document.body.textContent).not.toMatch(/(?<!not )done — /);
});

// ── What a command is, and where it sits ───────────────────────────────────
// A command is carried to a terminal whole; a value is typed into the field
// the sentence beside it names. Two readings, so two renderings.

describe("a command is a block, not a phrase inside a sentence", () => {
  it("leaves the sentence that introduces it and takes a surface of its own", () => {
    draw();
    fireEvent.click(within(task(1)).getByText(MANUAL));

    const block = commandNamed("the trust command");
    expect(block).toHaveTextContent("security add-trusted-cert");
    expect(block).not.toHaveTextContent("Run it in a terminal:");
    expect(screen.getByText("Run it in a terminal:")).toBeInTheDocument();
    expect(block.className).toContain("bg-canvas-soft");
  });

  it("is wire data in the mono family, whole and uncut", () => {
    draw();
    fireEvent.click(within(task(1)).getByText(MANUAL));

    const code = commandNamed("the trust command").querySelector("[data-wire]")!;
    expect(code).toHaveAttribute("data-wire", "shell command");
    expect(code.className).toContain("font-mono");
    expect(code.textContent).toBe(
      "sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain ~/Downloads/trawler-ca.pem",
    );
  });

  // W7's C1: once saved, the command names where the save put the file.
  it("names the file where the save put it", () => {
    draw({ savedCa: { pem: "/Users/me/My Certs/trawler-ca.pem" } });
    fireEvent.click(within(task(1)).getByText(MANUAL));

    expect(commandNamed("the trust command").querySelector("[data-wire]")!.textContent).toBe(
      "sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain '/Users/me/My Certs/trawler-ca.pem'",
    );
  });

  it("the try-it command is a command too, block and all", () => {
    draw();

    const block = commandNamed("the try-it command");
    expect(block).toHaveTextContent("curl -x http://127.0.0.1:9999 https://example.com");
    expect(block.className).toContain("bg-canvas-soft");
    expect(screen.getByText("or run this in a terminal")).toBeInTheDocument();
  });

  it("a value stays in the sentence that says which field it goes in", () => {
    draw();
    pick("Android emulator");

    const host = screen.getByRole("button", { name: /copy the emulator's host/i });
    expect(host).toHaveTextContent("10.0.2.2");
    expect(host.closest("li")).toHaveTextContent("server");
    expect(host.closest("li")).toHaveTextContent("port");
  });

  it("a caveat is a line under the steps, and never one of them", () => {
    draw();
    pick("Android emulator");

    const android =
      "Apps targeting Android 7+ only trust user CAs when their network security config opts in.";
    expect(screen.getAllByTestId("guide-note").map((note) => note.textContent)).toContain(android);
    const steps = within(task(1)).getAllByRole("listitem");
    expect(steps.map((step) => step.textContent).join(" ")).not.toContain("Apps targeting Android");
  });

  it("a second way in is behind its own summary, closed", () => {
    draw();
    pick("Android emulator");

    expect(screen.getByText("other ways")).toBeInTheDocument();
    expect(open("other ways")).toBe(false);
    expect(commandNamed("the emulator command")).toHaveTextContent("emulator -avd");
  });
});

// The rule the window round is built on, asserted as an invariant over every
// platform rather than on the two the cases above happen to walk. This WU's own
// review round 5 caught a regression test that held one input still and so only
// ever exercised one arm of the expression it was pinning; a rule stated per
// platform and checked on two of eight is the same shape.
describe("one guide open, whatever the user picked", () => {
  // A guide's summary is a count or the name of the other route. The summaries
  // inside one — other ways, the way back, a failure's details — are not guides.
  const guides = () =>
    [...document.querySelectorAll("details")].filter((entry) => {
      const summary = entry.querySelector("summary")?.textContent?.trim() ?? "";
      return /^\d+ steps?$/.test(summary) || summary === MANUAL;
    });
  const opened = () => guides().filter((entry) => entry.hasAttribute("open"));

  const atPlatform = (id: PlatformId, props: Props = {}) => {
    draw({ reach: ARMED, trustProbe: "absent", proxy: { status: "none", address: null }, ...props });
    pick(setup.platformLabel(id, "macos"));
  };

  it.each(PLATFORM_IDS)("%s opens at most one, and it is the first", (id) => {
    atPlatform(id);

    const open = opened();
    expect(open.length).toBeLessThanOrEqual(1);
    if (open.length === 1) expect(open[0]).toBe(guides()[0]);
  });

  it.each(PLATFORM_IDS)("%s never opens a route a button already answers", (id) => {
    atPlatform(id);

    for (const entry of guides()) {
      if (entry.querySelector("summary")?.textContent?.trim() === MANUAL) {
        expect(entry).not.toHaveAttribute("open");
      }
    }
  });

  it.each(PLATFORM_IDS)("%s opens none once a request has decrypted", (id) => {
    atPlatform(id, { traffic: capturing });

    expect(opened()).toHaveLength(0);
  });
  it("a guide the user closed does not follow them to the next platform (R11)", () => {
    draw({ os: "linux", reach: ARMED });
    pick("Another Mac");
    const closed = within(task(2)).getAllByRole("group")[0] as HTMLDetailsElement;
    expect(closed).toHaveAttribute("open");
    closed.open = false;

    pick(setup.platformLabel("this", "linux"));

    expect(within(task(1)).getAllByRole("group")[0]).toHaveAttribute("open");
  });
});

test("the reading column answers to the window, not to a dialog", () => {
  const { container } = draw();
  const column = container.firstElementChild!.firstElementChild!;

  expect(column.className).toContain("max-w-reading");
  expect(column.className).not.toContain("max-w-dialog-max");
});

describe("an action that changed nothing says so", () => {
  // The decline of the replace-a-foreign-proxy confirm resolves null rather than
  // rejecting, so the action lands in "nothing" and not "failed". Without a note
  // for that state the button re-enables with the same label and the screen is
  // byte-identical to one that was never clicked - the exact outcome ARM_NOTE's
  // own comment rules out for the reach confirm.
  const DECLINED_BOTH =
    "nothing changed — the other proxy is still the one in use, and the certificate was not installed.";
  const DECLINED_PROXY = "nothing changed — the other proxy is still the one in use.";

  it("the combined action names both halves it did not do", async () => {
    draw({ onSetUpThisMachine: vi.fn(async () => null) });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() => expect(screen.getByText(DECLINED_BOTH)).toBeInTheDocument());
  });

  it("the Simulator's proxy action names the one half it has", async () => {
    draw({ onSetProxy: vi.fn(async () => null) });
    pick("iOS Simulator");

    fireEvent.click(screen.getByRole("button", { name: "Set the system proxy" }));

    await waitFor(() => expect(screen.getByText(DECLINED_PROXY)).toBeInTheDocument());
  });

  it("neither reads as a failure — a decline is not the OS refusing", async () => {
    draw({ onSetUpThisMachine: vi.fn(async () => null) });

    fireEvent.click(screen.getByRole("button", { name: "Set up this Mac" }));

    await waitFor(() =>
      expect(screen.getByTestId("set-up-status")).toHaveAttribute("data-state", "nothing"),
    );
    expect(screen.queryByText(/setting up failed/)).toBeNull();
    expect(screen.queryByRole("group", { name: "details" })).toBeNull();
  });
});

test("a task is set apart from its neighbours by more than its own parts are", () => {
  const { container } = draw();
  const column = container.firstElementChild!.firstElementChild!;

  // A task separates its title, its reason and its state by 8px. Before the
  // column carried a rhythm of its own, 16px was all that separated one task
  // from the next — so three tasks read as one list of nine lines. The column
  // spaces its zones and the zone spaces its tasks, and both sit above 8px.
  expect(column.className).toContain("gap-6");
  expect(screen.getByTestId("steps").className).toContain("gap-4");

  // and the rhythm is the column's alone: a zone that brings a top padding of
  // its own is a second spacing rule nothing reconciles with the first.
  for (const zone of [...column.children]) {
    expect(zone.className).not.toMatch(/\bpt-/);
  }
});

test("the column scrolls whole — no zone holds an inner scroller", () => {
  const { container } = draw();
  const scrollers = container.querySelectorAll("[class*='overflow-auto'],[class*='overflow-y-auto']");
  expect(scrollers).toHaveLength(1);
  expect(scrollers[0]).toBe(container.firstElementChild);
});

test("the emulator's save control saves the DER its installer opens, and names it by that file", async () => {
  const onSaveCa = vi.fn(async () => "trawler-ca.cer");
  draw({ onSaveCa });
  pick("Android emulator");

  fireEvent.click(within(task(1)).getByRole("button", { name: "save…" }));

  expect(onSaveCa).toHaveBeenCalledWith("cer");
  expect(
    await within(task(1)).findByText("saved as trawler-ca.cer. now trust it with the steps here."),
  ).toBeInTheDocument();
});

test("an Android device whose requests arrive and do not decrypt is told where its user store is, and what ignores it", () => {
  draw({ reach: ARMED, traffic: arriving });
  pick("Android device");

  expect(within(task(2)).getByTestId("traffic-remedy")).toHaveTextContent(
    "something reached Trawler and did not decrypt — your Android device does not trust Trawler's certificate yet. The steps below install it. Already did? Trawler Proxy CA should be under Settings → Security → Encryption & credentials → Trusted credentials → User. Most apps ignore a user certificate on Android 7+; a website in Chrome is the test.",
  );
});

// ── The request the app makes of its own (F24 · DEC-56) ────────────────────

describe("make a request to example.com", () => {
  const REQUEST = "make a request to example.com";
  const states = () =>
    ["task-1-state", "task-2-state", "traffic-state"].map((id) => screen.getByTestId(id).textContent);
  const through = () => vi.fn(async (): Promise<RequestRoute> => "through");

  it("is This machine's third route, and nothing runs it but a click", () => {
    const onMakeRequest = through();
    draw({ onMakeRequest });

    expect(within(task(3)).getByRole("button", { name: REQUEST })).toBeInTheDocument();
    expect(within(task(3)).getByText("or run this in a terminal")).toBeInTheDocument();
    expect(onMakeRequest).not.toHaveBeenCalled();
  });

  it("is offered on no other device — its request travels through this host", () => {
    draw({ onMakeRequest: through(), reach: ARMED });
    for (const device of ["iOS Simulator", "Android emulator", "iPhone or iPad", "Another Mac"]) {
      pick(device);
      expect(screen.queryByRole("button", { name: REQUEST })).toBeNull();
    }
  });

  // D9: the note says which way the request went, and only a task's own
  // reading moves that task (DEC-56).
  it.each([
    ["through", "example.com answered through Trawler."],
    ["undecrypted", "the request reached Trawler and didn't decrypt."],
    ["around", "example.com answered, but not through Trawler."],
  ] as const)("says the request went %s, and leaves every task's state to its own reading", async (route, note) => {
    const onMakeRequest = vi.fn(async (): Promise<RequestRoute> => route);
    draw({ onMakeRequest, trustProbe: "trusted", proxy: { status: "ours", address: null } });
    const before = states();

    fireEvent.click(screen.getByRole("button", { name: REQUEST }));

    await waitFor(() => expect(screen.getByTestId("make-request-status")).toHaveTextContent(note));
    expect(onMakeRequest).toHaveBeenCalledOnce();
    expect(states()).toEqual(before);
    expect(task(3)).toHaveTextContent("not done — waiting for the first request");
  });

  it("names nothing that went well where the request went around Trawler", async () => {
    draw({ onMakeRequest: vi.fn(async (): Promise<RequestRoute> => "around") });

    fireEvent.click(screen.getByRole("button", { name: REQUEST }));

    await waitFor(() => expect(screen.getByTestId("make-request-status")).toHaveTextContent("not through Trawler"));
    expect(screen.getByTestId("make-request-status")).not.toHaveTextContent(/^example.com answered\.$/);
  });

  it("says a request that did not complete, and keeps the manual routes", async () => {
    draw({ onMakeRequest: vi.fn(() => Promise.reject(new TypeError("Load failed"))) });

    fireEvent.click(screen.getByRole("button", { name: REQUEST }));

    await waitFor(() =>
      expect(screen.getByTestId("make-request-status")).toHaveTextContent(
        "the request to example.com didn't complete.",
      ),
    );
    expect(screen.getByRole("button", { name: REQUEST })).toBeEnabled();
    expect(screen.getByRole("button", { name: "the try-it command" })).toBeInTheDocument();
  });
});

// ── This machine where no probe answers: its own request (D6 · D7 · DEC-62) ──

describe("This machine where no probe answers", () => {
  const unread = { trustProbe: "unknown", proxy: { status: "unknown", address: null } } as const;
  const onMakeRequest = vi.fn(async (): Promise<RequestRoute> => "through");

  it("names the request in step 3 as what settles both tasks", () => {
    draw({ os: "linux", ...unread, onMakeRequest });

    expect(task(1)).toHaveTextContent("can't check here — the request in step 3 confirms it");
    expect(task(2)).toHaveTextContent("can't check here — the same request confirms it");
    expect(within(task(3)).getByRole("button", { name: "make a request to example.com" })).toBeInTheDocument();
  });

  // D6: the request travels this host's system proxy and is checked against
  // this host's store, so one that came through decrypted settles both.
  it("reads both tasks done once its own request came through, with the time it did", () => {
    const at = Date.now() - 60_000;
    draw({ os: "linux", ...unread, onMakeRequest, ownProofAt: at });

    expect(task(1)).toHaveTextContent(`done — step 3's request decrypted at ${clock(at)}`);
    expect(task(2)).toHaveTextContent(`done — step 3's request came through at ${clock(at)}`);
    expect(within(task(1)).queryByText("3 steps")).toBeNull();
    expect(within(task(2)).queryByText("1 step")).toBeNull();
  });

  // D7: a phone still pointed here decrypts, and says nothing about this machine.
  it("is not settled by another device's decrypted request", () => {
    draw({ os: "linux", ...unread, onMakeRequest, traffic: capturing });

    expect(task(3)).toHaveTextContent("done — capturing traffic");
    expect(task(1)).toHaveTextContent("can't check here — the request in step 3 confirms it");
    expect(task(2)).toHaveTextContent("can't check here — the same request confirms it");
  });

  it("leaves a refusal nobody can place to the try task, and never pins it on this machine", () => {
    draw({ os: "linux", ...unread, onMakeRequest, traffic: { ...NO_TRAFFIC, lastTunnelAt: ago(TUNNEL_GRACE_MS + 1_000), openTunnelAt: ago(TUNNEL_GRACE_MS + 1_000) } });

    expect(task(1)).toHaveTextContent("can't check here — the request in step 3 confirms it");
    expect(task(1)).not.toHaveTextContent("does not trust Trawler's certificate");
    expect(task(3)).toHaveTextContent("not working — arriving but not decrypting");
    expect(within(task(3)).getByTestId("traffic-remedy")).toHaveTextContent(
      "the device it comes from does not trust Trawler's certificate yet",
    );
  });

  it("is outranked by any probe that answers", () => {
    draw({
      os: "linux",
      trustProbe: "absent",
      proxy: { status: "none", address: null },
      onMakeRequest,
      ownProofAt: Date.now(),
    });

    expect(task(1)).toHaveTextContent("not done — not installed");
    expect(task(2)).toHaveTextContent("not done — the system proxy is off");
  });

  it("stands for This machine only: the Simulator's proxy still promises its own first request", () => {
    draw({ ...unread, ownProofAt: Date.now() });
    pick("iOS Simulator");

    expect(task(2)).toHaveTextContent("can't check here — the Simulator's first request confirms it");
  });

  // The promise rule's find: that request used to move nothing where this Mac's
  // proxy could not be read.
  it("keeps the Simulator's promise: a request reaching Trawler settles its route", () => {
    const plain = { ...NO_TRAFFIC, lastPlainAt: Date.now() };
    draw({ ...unread, device: { provedAt: null, mixed: true, wire: plain } });
    pick("iOS Simulator");

    expect(task(2)).toHaveTextContent("done — requests reached Trawler");
  });
});

// ── A code for the value a phone cannot paste (F28 · DEC-57) ────────────────

describe("the certificate URL's scannable code", () => {
  it.each([
    ["iPhone or iPad", ARMED.ca_urls!.mobileconfig],
    ["Android device", ARMED.ca_urls!.cer],
  ])("on %s, sits under the step that prints the URL, and encodes exactly that URL", (device, url) => {
    draw({ reach: ARMED });
    pick(device);

    const code = screen.getByRole("img", { name: "the certificate URL, as a code to scan" });
    expect(code).toHaveAttribute("data-value", url);
    const step = code.closest("li");
    expect(step).toHaveTextContent(url);
    expect(within(step as HTMLElement).getByRole("button", { name: "copy the certificate URL" })).toBeInTheDocument();
    expect(screen.getAllByTestId("scan-code")).toHaveLength(1);
  });

  it("is drawn for no value a scan cannot use — the proxy host and port stay typed", () => {
    draw({ reach: ARMED });
    pick("iPhone or iPad");

    for (const code of screen.getAllByRole("img", { name: /as a code to scan/ }))
      expect(code.getAttribute("data-value")).not.toBe(ARMED.proxy);
  });

  it("waits for an address — before reach is armed there is nothing to encode", () => {
    draw({ reach: UNARMED });
    pick("iPhone or iPad");

    expect(screen.queryByTestId("scan-code")).toBeNull();
  });

  it.each(["Another Mac", "Another Windows PC", "Another Ubuntu machine", "Android emulator", "iOS Simulator"])(
    "is not drawn for %s, which opens the URL without a camera",
    (device) => {
      draw({ reach: ARMED });
      pick(device);

      expect(screen.queryByTestId("scan-code")).toBeNull();
    },
  );
});
