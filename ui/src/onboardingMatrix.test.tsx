import { afterEach, describe, expect, test } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { Onboarding, useRouteMemory } from "./Onboarding";
import type { OnbState } from "./onboardingMachine";
import type { ReachState } from "./api";
import {
  canAutoSetup,
  deviceName,
  platformChoices,
  platformLabel,
  setupFor,
  type HostOs,
  type PlatformId,
  type ReachAddresses,
} from "./setup";
import {
  mayCarryOwnTraffic,
  NO_TRAFFIC,
  RECENT_MS,
  STATE_WORDS,
  TUNNEL_GRACE_MS,
  type EmulatorReading,
  type ProxyProbe,
  type SimulatorTrust,
  type Traffic,
  type TrustProbe,
} from "./verdicts";

// The state sweep of 2026-09-25, kept and widened. Every state the setup screen
// can be in is rendered and flattened into an outline - each task's state line,
// its actions, its commands, its guides and which are open, its notices, and
// anything drawn between tasks - and the outline is judged against a model of
// what each task's reading is, written here independently of the screen:
//
//   certificate  a probe that concludes (trusted / installed / absent) is the reading;
//                on This machine where it does not, its own request is (D6),
//                and no other device's traffic (D7); elsewhere the traffic is -
//                a decrypted request is done, a refusal with none decrypted is
//                not done, and nothing yet is a promise to confirm on the first
//                HTTPS request
//   proxy        where a probe exists (this machine, the Simulator) it is the
//                reading, and This machine's own request where it cannot
//                answer; elsewhere any request that reached Trawler is done
//   another device  its marked request settles both, named; other traffic
//                counts only where this machine's proxy cannot be sending its
//                own here too - read off, or another proxy (DEC-63). Each
//                proof carries its time (P2), and its try task keeps it past
//                whatever follows that does not decrypt (S1)
//   the Simulator  its wire is this machine's, always: its certificate is the
//                store of each Simulator running, else its marked request; its
//                try task and the end read the mark only (W7)
//   the emulator  another device that needs no allow: its own store and proxy
//                as adb reads them, else what another device's tasks read; a
//                refusal is never its certificate task's (W8)
//   a phone      another device whose test is its address: only that address
//                refused, the phone's and newer than any proof, is its
//                certificate's to say; any other refusal is the try task's
//                (the iPhone W9, the Android device W10)
//
// and against three kinds of rule. PLACE, an instruction sits inside the task
// it is for; TIME, an action or a step is offered only while its task is not
// read done; TRUTH, no sentence contradicts a reading on the same screen.

// jsdom fires a <details> element's toggle from a timer it queues when the
// element opens, and the timer holds the element's whole rendered tree until it
// runs. A synchronous test never yields to it, so a sweep of them keeps every
// open guide it drew, and outgrows a worker's heap unless each test drains it.
const drainingToggles = () =>
  afterEach(() => new Promise((resolve) => setTimeout(resolve, 0)));

const HOSTS: HostOs[] = ["macos", "windows", "linux", "other"];
const TRUST: TrustProbe[] = ["trusted", "installed", "absent", "unknown"];
const PROXY: ProxyProbe[] = ["ours", "foreign", "none", "unknown"];

// Traffic is a history, not a moment: the nine shapes the evidence can take.
const HISTORIES = [
  "none",
  "plain",
  "tunnel-in-grace",
  "refused",
  "refused-and-retrying",
  "capturing",
  "decrypted-earlier",
  "refused-earlier",
  "decrypted-then-refused",
] as const;
type History = (typeof HISTORIES)[number];

// The histories whose newest tunnel no decrypted request answered.
const UNANSWERED = new Set<History>([
  "refused",
  "refused-and-retrying",
  "refused-earlier",
  "decrypted-then-refused",
]);

const EARLIER = RECENT_MS * 2;
const PAST_GRACE = TUNNEL_GRACE_MS + 2_000;

function trafficOf(history: History): Traffic {
  const ago = (ms: number) => Date.now() - ms;
  switch (history) {
    case "none":
      return NO_TRAFFIC;
    case "plain":
      return { ...NO_TRAFFIC, lastPlainAt: ago(1_000) };
    case "tunnel-in-grace":
      return { ...NO_TRAFFIC, lastTunnelAt: ago(0), openTunnelAt: ago(0) };
    case "refused":
      return {
        ...NO_TRAFFIC,
        lastTunnelAt: ago(PAST_GRACE),
        openTunnelAt: ago(PAST_GRACE),
      };
    case "refused-and-retrying":
      return {
        ...NO_TRAFFIC,
        lastTunnelAt: ago(0),
        openTunnelAt: ago(PAST_GRACE),
      };
    case "capturing":
      return { ...NO_TRAFFIC, lastDecryptedAt: ago(1_000) };
    case "decrypted-earlier":
      return { ...NO_TRAFFIC, lastDecryptedAt: ago(EARLIER) };
    case "refused-earlier":
      return {
        ...NO_TRAFFIC,
        lastTunnelAt: ago(EARLIER),
        openTunnelAt: ago(EARLIER),
      };
    case "decrypted-then-refused":
      return {
        ...NO_TRAFFIC,
        lastDecryptedAt: ago(EARLIER),
        lastTunnelAt: ago(PAST_GRACE),
        openTunnelAt: ago(PAST_GRACE),
      };
  }
}

const ANY_REACH: ReachAddresses = {
  proxy: "192.168.1.24:7420",
  caUrls: {
    pem: "http://192.168.1.24:7421/ca.pem",
    cer: "http://192.168.1.24:7421/ca.cer",
    mobileconfig: "http://192.168.1.24:7421/ca.mobileconfig",
  },
};

const OFF_MACHINE = new Set<PlatformId>([
  "mac",
  "windows",
  "ubuntu",
  "ios",
  "android",
]);

// The sweep's trust probe, as the Simulator probe reads it: a running
// Simulator's store holding the certificate or not, one whose store did not
// answer, or none running.
const RUNNING: Record<TrustProbe, SimulatorTrust[]> = {
  trusted: [{ name: "iPhone 17 Pro", trust: "trusted" }],
  absent: [{ name: "iPhone 17 Pro", trust: "absent" }],
  installed: [{ name: "iPhone 17 Pro", trust: "unknown" }],
  unknown: [],
};

// The emulator's readings, swept on the same axis: its store holding the
// certificate and its proxy at Trawler, neither and its proxy on a network it
// does not send over, a store and a proxy adb could not read, or none running.
const EMULATOR = "Pixel 7 API 35";
const EMULATORS: Record<TrustProbe, EmulatorReading[]> = {
  trusted: [{ name: EMULATOR, trust: "trusted", proxy: "ours" }],
  absent: [{ name: EMULATOR, trust: "absent", proxy: "wifi-only" }],
  installed: [{ name: EMULATOR, trust: "unknown", proxy: "none" }],
  unknown: [],
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
  interfaces: [{ name: "Wi-Fi", address: "192.168.1.24" }],
};
const UNARMED: ReachState = {
  ...ARMED,
  armed: false,
  proxy: null,
  api: null,
  ca_urls: null,
};

type Outcome =
  "done" | "nothing" | "failed" | "refused" | "unshowable" | "pending";
const OUTCOMES: Outcome[] = [
  "done",
  "nothing",
  "failed",
  "refused",
  "unshowable",
];

const onboarding: OnbState = {
  phase: "onboarding",
  step: "trust-ca",
  everProven: false,
  since: null,
  chosen: "this",
  another: false,
};

const settle = (outcome: Outcome) => () => {
  if (outcome === "pending") return new Promise<never>(() => {});
  if (outcome === "done") return Promise.resolve();
  if (outcome === "nothing") return Promise.resolve(null);
  return Promise.reject({
    refused: outcome === "refused",
    unshowable: outcome === "unshowable",
    detail: "scripted",
  });
};

interface State {
  os: HostOs;
  platform: PlatformId;
  armed: boolean;
  trust: TrustProbe;
  proxy: ProxyProbe;
  history: History;
  // This machine's own request came through, decrypted, in this session.
  own: boolean;
  // Another device's marked request came through, decrypted, since its
  // address was allowed (DEC-63).
  marked: boolean;
  // The history's open tunnel is to the host of the address a phone is
  // offered, rather than one of its own services (W9, W10).
  address: boolean;
}

const OWN_AT = Date.now() - 60_000;
const MARK = "k3m9";
// Older than the window of "now", newer than a refusal from earlier: so a mark
// answers that tunnel, and never one that opened after it.
const MARKED_AGO = 45_000;

// The wire as the screen is handed it: the marked request is a decrypted row
// like any other, taken at render time so a long sweep never ages it past the
// tunnels it is ordered against.
function wireOf(s: State): { traffic: Traffic; markedAt: number | null } {
  const traffic = trafficOf(s.history);
  if (!s.marked) return { traffic, markedAt: null };
  const markedAt = Date.now() - MARKED_AGO;
  const lastDecryptedAt = Math.max(
    traffic.lastDecryptedAt ?? markedAt,
    markedAt,
  );
  const openTunnelAt =
    traffic.openTunnelAt !== null && traffic.openTunnelAt > lastDecryptedAt
      ? traffic.openTunnelAt
      : null;
  return { traffic: { ...traffic, lastDecryptedAt, openTunnelAt }, markedAt };
}

type Handlers = Partial<
  Record<"setUp" | "setProxy" | "unset" | "trustSimulators", Outcome>
>;

// Nothing here asserts on a handler, so none is a mock: every mock is kept for
// the whole file, and twenty thousand renders' worth outgrows a worker's heap.
const nothing = () => {};
const nothingSaved = async () => null;
const nothingAsync = async () => {};
const through = async () => "through" as const;

// App holds what the shell last did to the proxy (S5); the host holds it the same way.
// It hands another device what could have come from it (DEC-63): while this
// machine's own traffic may be arriving, which is when the sweep's wire arrived,
// that is the marked request alone.
function Host({ s, handlers }: { s: State; handlers: Handlers }) {
  const [routeAct, onRouteAct] = useRouteMemory(s.proxy);
  const { traffic, markedAt } = wireOf(s);
  const mixed =
    s.platform === "ios-sim" ||
    ((OFF_MACHINE.has(s.platform) || s.platform === "android-sim") &&
      mayCarryOwnTraffic(s.proxy));
  const its = !mixed
    ? traffic
    : markedAt === null
      ? NO_TRAFFIC
      : { ...NO_TRAFFIC, lastDecryptedAt: markedAt };
  const refusal =
    s.address && traffic.openTunnelAt !== null
      ? { host: "example.com", at: traffic.openTunnelAt, mine: !mixed }
      : undefined;
  return (
    <Onboarding
      state={onboarding}
      dispatch={nothing}
      os={s.os}
      proxyAddr="127.0.0.1:9999"
      trustProbe={s.trust}
      proxy={{
        status: s.proxy,
        address: s.proxy === "foreign" ? "10.1.1.1:8080" : null,
      }}
      traffic={its}
      ownProofAt={s.own ? OWN_AT : null}
      simulators={RUNNING[s.trust]}
      emulators={EMULATORS[s.trust]}
      deviceMark={MARK}
      device={{ provedAt: markedAt, mixed, wire: traffic, refusal }}
      reach={s.armed ? ARMED : UNARMED}
      onSaveCa={nothingSaved}
      onSetUpThisMachine={settle(handlers.setUp ?? "done")}
      onSetUpSimulator={settle(handlers.setUp ?? "done")}
      onTrustSimulators={
        settle(handlers.trustSimulators ?? "done") as () => Promise<void>
      }
      onSetProxy={settle(handlers.setProxy ?? "done")}
      onUnsetProxy={settle(handlers.unset ?? "done") as () => Promise<void>}
      onArmReach={nothingAsync}
      onDisarmReach={nothingAsync}
      onMakeRequest={through}
      routeAct={routeAct}
      onRouteAct={onRouteAct}
    />
  );
}

function draw(s: State, handlers: Handlers = {}) {
  render(<Host s={s} handlers={handlers} />);
  if (s.platform !== "this") {
    const question = "which device's traffic do you want to see";
    fireEvent.click(screen.getByRole("button", { name: question }));
    fireEvent.click(
      screen.getByRole("option", { name: platformLabel(s.platform, s.os) }),
    );
  }
}

// ── The outline ─────────────────────────────────────────────────────────────

const INSTALL_COMMAND = {
  trust: "the trust command",
  route: "the proxy command",
} as const;
const REVERSAL = "stop routing through Trawler";
const WAY_BACK = new Set([
  "to remove it later",
  "to turn it off later",
  "do it by hand",
  "details",
]);
const UNDO_SUMMARIES = new Set(["to remove it later", "to turn it off later"]);
// A sentence that sends the reader to the steps under it (W12's W1).
const POINTS = /the steps below|do it by hand below/i;
const ACTION_LABEL = /^(set up this|set the system proxy|stop routing)/;

interface TaskOutline {
  n: number;
  state: string;
  text: string;
  actions: string[];
  commands: string[];
  guides: string[];
  openGuides: string[];
  refusals: number;
}

interface Outline {
  tasks: TaskOutline[];
  oneAct: string | null;
  framed: string[];
  labels: string[];
  between: string[];
  proven: string | null;
  lines: string[];
  notes: string[];
}

const GLYPH = /^[✓○⚠◐⟳]?\s*/;
const textOf = (el: Element) =>
  (el.textContent ?? "").replace(GLYPH, "").trim();

const topLevel = (details: Element) =>
  details.parentElement?.closest("details") === null;
const summaryOf = (d: Element) =>
  d.querySelector("summary")?.textContent?.replace("›", "").trim() ?? "";

function outline(): Outline {
  const steps = screen.getByTestId("steps");
  const children = [...steps.children];
  const isTask = (el: Element) =>
    /^task-\d$/.test(el.getAttribute("data-testid") ?? "");
  return {
    tasks: children.filter(isTask).map((el) => {
      const n = Number(el.getAttribute("data-testid")!.slice("task-".length));
      const guides = [...el.querySelectorAll("details")].filter(topLevel);
      return {
        n,
        state:
          el.querySelector(`[data-testid="task-${n}-state"]`)?.textContent ??
          "",
        text: el.textContent ?? "",
        actions: [...el.querySelectorAll("button")]
          .map((b) => b.textContent?.trim() ?? "")
          .filter((label) => ACTION_LABEL.test(label)),
        commands: [
          ...el.querySelectorAll('button[aria-label]:not([aria-label^="help:"])'),
        ].map(
          (b) => b.getAttribute("aria-label") ?? "",
        ),
        guides: guides.map(summaryOf),
        openGuides: guides.filter((d) => d.hasAttribute("open")).map(summaryOf),
        refusals: [
          ...el.querySelectorAll('[data-testid="traffic-remedy"]'),
        ].filter((r) => /decrypt/.test(r.textContent ?? "")).length,
      };
    }),
    oneAct: screen.queryByTestId("one-act")?.textContent ?? null,
    // A framed control is a word button in its border; copy buttons name
    // themselves by aria-label and frame nothing the reader acts on next.
    framed: [...steps.querySelectorAll("button:not([aria-label])")]
      .filter((b) => b.className.includes("border-hairline"))
      .map((b) => b.textContent?.trim() ?? ""),
    labels: [...steps.querySelectorAll("button:not([aria-label])")].map(
      (b) => b.textContent?.trim() ?? "",
    ),
    between: children
      .filter(
        (el) => !isTask(el) && el.getAttribute("data-testid") !== "one-act",
      )
      .map((el) => el.textContent ?? ""),
    proven: screen.queryByTestId("proven")?.textContent ?? null,
    lines: [...steps.querySelectorAll('[data-testid$="-state"]')].map(textOf),
    notes: [
      ...steps.querySelectorAll(
        '[data-testid$="-status"] > :first-child, [data-testid="traffic-remedy"]',
      ),
    ].map(textOf),
  };
}

// ── The model ───────────────────────────────────────────────────────────────

function model(s: State) {
  const probesTrust = s.platform === "this";
  const probesRoute = s.platform === "this" || s.platform === "ios-sim";
  const needsReach = OFF_MACHINE.has(s.platform);
  const first = needsReach ? 1 : 0;
  const h = s.history;
  // The wire, the marked request in it: the mark answers a refusal from before
  // it, never one after it.
  const onWire = {
    reached: s.marked || h !== "none",
    decrypted:
      s.marked ||
      h === "capturing" ||
      h === "decrypted-earlier" ||
      h === "decrypted-then-refused",
    unanswered:
      h === "refused" ||
      h === "refused-and-retrying" ||
      h === "decrypted-then-refused" ||
      (h === "refused-earlier" && !s.marked),
  };
  // DEC-63: where this machine's own traffic may be arriving too, only the
  // marked request is another device's evidence - the Simulator's, always.
  const sharedWire = s.platform === "ios-sim";
  const emulator = s.platform === "android-sim";
  const phone = s.platform === "ios" || s.platform === "android";
  const anotherDevice = needsReach || emulator;
  const mixed =
    sharedWire ||
    (anotherDevice && (s.proxy === "ours" || s.proxy === "unknown"));
  // Every open tunnel the sweep draws is newer than every proof beside it.
  const addressRefused = phone && s.address && onWire.unanswered && !mixed;
  const reached = mixed ? s.marked : onWire.reached;
  const decrypted = mixed ? s.marked : onWire.decrypted;
  const unanswered = mixed ? false : onWire.unanswered;
  const refused = unanswered && !decrypted;
  const trustConcluded = probesTrust && s.trust !== "unknown";
  const ownStore = !sharedWire
    ? null
    : s.trust === "trusted"
      ? true
      : s.trust === "absent"
        ? false
        : null;
  const emulatorStore = !emulator
    ? null
    : s.trust === "trusted"
      ? true
      : s.trust === "absent"
        ? false
        : null;
  const emulatorRouted = !emulator
    ? null
    : s.trust === "trusted"
      ? true
      : s.trust === "absent"
        ? false
        : null;
  const trustDone = sharedWire
    ? (ownStore ?? s.marked)
    : emulator
      ? (emulatorStore ?? decrypted)
      : !probesTrust
        ? decrypted && !addressRefused
        : trustConcluded
          ? s.trust === "trusted"
          : s.own;
  // The Simulator's route is this machine's proxy, so anything on the wire
  // reached Trawler through it.
  const routeReached = sharedWire ? onWire.reached : reached;
  const routeDone =
    probesRoute && s.proxy !== "unknown"
      ? s.proxy === "ours"
      : s.platform === "this"
        ? s.own
        : (emulatorRouted ?? routeReached);
  // A refusal is the certificate task's while it is not done - except where
  // the refusal may be anyone's: This machine's store unread (D7), another
  // device's traffic mixed with this machine's (A3), and the emulator, whose
  // own services refuse a certificate it holds (W8).
  const certificateOwnsRefusal = phone
    ? addressRefused
    : !trustDone && !(probesTrust && !trustConcluded) && !mixed && !emulator;
  const auto = canAutoSetup(s.os);
  const routeHere = probesRoute && auto;
  // A Simulator runs wherever the sweep reads a store, trusted, absent or not
  // answering; "unknown" is none running (RUNNING).
  const simulatorRunning = sharedWire && s.trust !== "unknown";
  const oneAct =
    (probesTrust || simulatorRunning) && auto && !trustDone && !routeDone;
  const trustSimulators = simulatorRunning && !oneAct && !trustDone;
  // What the try task's clock says now: a refusal inside the window is "not
  // decrypting", however new the tunnel after it; one before it is quiet.
  const notDecryptingNow =
    h === "refused" ||
    h === "refused-and-retrying" ||
    h === "decrypted-then-refused";
  return {
    ...{
      probesTrust,
      probesRoute,
      first,
      reached,
      routeReached,
      decrypted,
      unanswered,
      refused,
      mixed,
      sharedWire,
    },
    ...{ emulator, anotherDevice, phone, addressRefused },
    trustN: first + 1,
    routeN: first + 2,
    tryN: first + 3,
    blocked: needsReach && !s.armed,
    trustConcluded,
    trustDone,
    routeDone,
    certificateOwnsRefusal,
    routeHere,
    oneAct,
    trustSimulators,
    notDecryptingNow,
    here: deviceName("this", s.os),
  };
}

type Rule = (
  o: Outline,
  s: State,
  extra: { reversal: boolean },
) => string | null;

const task = (o: Outline, n: number) => o.tasks.find((t) => t.n === n)!;
const withCommand = (o: Outline, name: string) =>
  o.tasks.filter((t) => t.commands.includes(name)).map((t) => t.n);
const withAction = (o: Outline, prefix: string) =>
  o.tasks
    .filter((t) => t.actions.some((a) => a.startsWith(prefix)))
    .map((t) => t.n);
const same = (a: number[], b: number[]) =>
  a.length === b.length && a.every((x, i) => x === b[i]);

function expectedTrustState(s: State): RegExp {
  const m = model(s);
  if (m.sharedWire) {
    if (s.trust === "trusted") return /^✓\s*done — trusted in iPhone 17 Pro$/;
    if (s.trust === "absent") return /^○\s*not done — not in iPhone 17 Pro$/;
    if (s.marked)
      return /^✓\s*done — the Simulator's request decrypted at \d\d:\d\d$/;
    if (s.trust === "unknown") return /^○\s*not done — no Simulator running$/;
    return new RegExp(
      `^◐\\s*can't check here — the address in task ${m.tryN} confirms it$`,
    );
  }
  if (m.emulator) {
    if (s.trust === "trusted") return /^✓\s*done — trusted in Pixel 7 API 35$/;
    if (s.trust === "absent") return /^○\s*not done — not in Pixel 7 API 35$/;
    if (s.marked)
      return /^✓\s*done — the emulator's request decrypted at \d\d:\d\d$/;
    if (m.decrypted) return /^✓\s*done — a request decrypted at \d\d:\d\d$/;
    if (m.mixed)
      return new RegExp(
        `^◐\\s*can't check here — the address in task ${m.tryN} confirms it$`,
      );
    return /^◐\s*can't check here — the emulator's first HTTPS request confirms it$/;
  }
  if (m.phone && !m.blocked) {
    const device = deviceName(s.platform, s.os);
    if (m.addressRefused)
      return m.decrypted
        ? /^⚠\s*not working — example\.com did not decrypt at \d\d:\d\d, after decrypting at \d\d:\d\d$/
        : /^⚠\s*not working — example\.com did not decrypt$/;
    if (s.marked)
      return new RegExp(
        `^✓\\s*done — ${device}'s request decrypted at \\d\\d:\\d\\d$`,
      );
    if (m.mixed)
      return /^◐\s*can't check here — the address in task 4 confirms it$/;
    if (m.decrypted) return /^✓\s*done — a request decrypted at \d\d:\d\d$/;
    if (m.refused)
      return /^◐\s*can't check here — the address in task 4 confirms it$/;
    return new RegExp(
      `^◐\\s*can't check here — ${device}'s first HTTPS request confirms it$`,
    );
  }
  if (m.probesTrust && !m.trustConcluded)
    return s.own
      ? /^✓\s*done — task 3's request decrypted at \d\d:\d\d$/
      : /^◐\s*can't check here — the request in task 3 confirms it$/;
  if (m.trustConcluded)
    return {
      trusted: browsersUnprobed(s)
        ? /^✓\s*done — trusted by the system's tools$/
        : /^✓\s*done — installed and trusted$/,
      installed: /^○\s*not done — installed, not trusted$/,
      absent: /^○\s*not done — not installed$/,
    }[s.trust as "trusted" | "installed" | "absent"];
  if (s.marked) return /^✓\s*done — .+'s request decrypted at \d\d:\d\d$/;
  if (m.mixed)
    return /^◐\s*can't check here — the address in task 4 confirms it$/;
  if (m.decrypted && OFF_MACHINE.has(s.platform))
    return /^✓\s*done — a request decrypted at \d\d:\d\d$/;
  if (m.decrypted) return /^✓\s*done — a request decrypted$/;
  if (m.refused) return /^⚠\s*not working — nothing decrypted$/;
  return /^◐\s*can't check here — .+'s first HTTPS request confirms it$/;
}

// This Linux machine's probe reads the system's store only, and its browsers
// keep their own: the done task says so and keeps their steps (W12's W6).
const browsersUnprobed = (s: State) =>
  s.platform === "this" && (s.os === "linux" || s.os === "other");
const STEP_COUNT = /^\d+ steps?$/;

function expectedRouteState(s: State): RegExp {
  const m = model(s);
  const another = m.anotherDevice;
  if (m.emulator && s.trust === "trusted")
    return /^✓\s*done — Pixel 7 API 35 routes through Trawler$/;
  if (m.emulator && s.trust === "absent")
    return /^○\s*not done — Pixel 7 API 35 sends over mobile data, not Wi-Fi$/;
  if (another && s.marked)
    return /^✓\s*done — .+'s request came through at \d\d:\d\d$/;
  if (another && m.mixed)
    return /^◐\s*can't check here — the same request confirms it$/;
  if (another && m.reached)
    return /^✓\s*done — a request reached Trawler at \d\d:\d\d$/;
  if (!m.probesRoute || (s.proxy === "unknown" && s.platform !== "this"))
    return m.routeReached
      ? /^✓\s*done — requests reached Trawler$/
      : /^◐\s*can't check here — .+'s first request confirms it$/;
  if (s.proxy === "ours") return /^✓\s*done — .+ routes through Trawler$/;
  if (s.proxy === "foreign") return /^○\s*not done — another proxy is set$/;
  if (s.proxy === "none") return /^○\s*not done — the system proxy is off$/;
  if (s.platform === "this")
    return s.own
      ? /^✓\s*done — task 3's request came through at \d\d:\d\d$/
      : /^◐\s*can't check here — the same request confirms it$/;
  return /^◐\s*can't check here — .+'s first request confirms it$/;
}

// F19: a state line's reason and a note are each a sentence, and a sentence is
// said once. A bare state word is a state, not a sentence — two tasks a write
// covers may both read "setting up".
function repeated(o: Outline): string[] {
  const reasons = o.lines
    .map((line) => line.split(" — ").slice(1).join(" — "))
    .filter(Boolean);
  const sentences = [
    ...reasons,
    ...o.notes,
    ...o.notes.flatMap((n) => (o.lines.includes(n) ? [n] : [])),
  ];
  return [...new Set(sentences.filter((x, i) => sentences.indexOf(x) !== i))];
}

const SAID_ONCE: Record<string, Rule> = {
  "truth: every state line opens with a state word (F5)": (o) => {
    const wordless = o.lines.filter(
      (line) =>
        !STATE_WORDS.some((w) => line === w || line.startsWith(`${w} — `)),
    );
    return wordless.length === 0
      ? null
      : `lines without a state word: ${wordless.join(" | ")}`;
  },

  "place: no sentence is said twice (F19)": (o) => {
    const twice = repeated(o);
    return twice.length === 0 ? null : `said twice: ${twice.join(" | ")}`;
  },
};

const RULES: Record<string, Rule> = {
  ...SAID_ONCE,
  "truth: the certificate task reads what its reading says": (o, s) => {
    const m = model(s);
    if (m.blocked) return null;
    const line = task(o, m.trustN).state;
    return expectedTrustState(s).test(line)
      ? null
      : `task ${m.trustN} reads "${line}"`;
  },

  "truth: the proxy task reads what its reading says": (o, s) => {
    const m = model(s);
    if (m.blocked) return null;
    const line = task(o, m.routeN).state;
    return expectedRouteState(s).test(line)
      ? null
      : `task ${m.routeN} reads "${line}"`;
  },

  "place: a refusal is said once - in the certificate task while it owns it, else in the try task while nothing decrypts now":
    (o, s) => {
      const m = model(s);
      const expected = m.blocked
        ? []
        : m.certificateOwnsRefusal && m.unanswered
          ? [m.trustN]
          : !m.certificateOwnsRefusal && m.notDecryptingNow
            ? [m.tryN]
            : [];
      const found = o.tasks.flatMap(
        (t) => Array(t.refusals).fill(t.n) as number[],
      );
      return same(found, expected)
        ? null
        : `refusal in tasks [${found}], expected [${expected}]`;
    },

  "truth: every sentence that points at the steps below has them open": (
    o,
    s,
  ) => {
    const pointing = o.tasks.filter((t) => POINTS.test(t.text));
    const shut = pointing
      .filter((t) => t.openGuides.length === 0)
      .map((t) => t.n);
    if (
      o.oneAct !== null &&
      POINTS.test(o.oneAct) &&
      task(o, model(s).trustN).openGuides.length === 0
    )
      shut.push(model(s).trustN);
    return shut.length === 0
      ? null
      : `tasks [${shut}] point at steps left shut`;
  },

  "place: nothing is drawn between the tasks but the one act above them": (
    o,
  ) =>
    o.between.length === 0 ? null : `between tasks: ${o.between.join(" | ")}`,

  "emphasis: one framed control at most, and it is the next thing to do (W12's E1, E2; W16's E2)":
    (o, s) => {
      const m = model(s);
      if (o.framed.length > 1) return `framed: ${o.framed.join(", ")}`;
      // The Simulator's task 1 outranks task 2 while not done: its button, or
      // its open guide's route, which the sweep draws without the shell's open.
      // A failure that opened task 2's steps instead leaves the proxy's framed.
      const trustGuideLeads =
        m.sharedWire &&
        !m.trustDone &&
        task(o, m.trustN).openGuides.length > 0;
      if (!m.oneAct && !m.trustSimulators && trustGuideLeads)
        return o.framed.length === 0
          ? null
          : `framed [${o.framed}], expected none`;
      const expected = m.oneAct
        ? /^set up /
        : m.trustSimulators
          ? /^trust in the Simulator$/
          : m.routeHere && !m.routeDone
            ? /^set the system proxy/
            : null;
      if (expected === null) return null;
      return o.framed.length === 1 && expected.test(o.framed[0])
        ? null
        : `framed [${o.framed}], expected ${expected}`;
    },

  "words: every platform's controls are lowercase (W12's W5, W13's W4)": (o) => {
    const capital = o.labels.filter((label) => /^[A-Z]/.test(label));
    return capital.length === 0 ? null : `capitalised: ${capital.join(", ")}`;
  },

  "words: a host with a name of its own is never this machine (W13's W3)": (o, s) => {
    if (s.os !== "macos" && s.os !== "windows") return null;
    const said = o.tasks.filter((t) => /this machine/.test(t.text)).map((t) => t.n);
    return said.length === 0 ? null : `"this machine" in task ${said.join(", ")}`;
  },

  "time: a task not done offers no way to undo it (W12's E4)": (o, s) => {
    const m = model(s);
    const undone = [
      ...(m.trustDone ? [] : [m.trustN]),
      ...(m.routeDone ? [] : [m.routeN]),
    ].map((n) => task(o, n));
    const wrong = undone.filter((t) =>
      t.guides.some((g) => UNDO_SUMMARIES.has(g)),
    );
    return wrong.length === 0
      ? null
      : wrong
          .map((t) => `task ${t.n} not done offers [${t.guides}]`)
          .join("; ");
  },

  "place: the certificate's install command sits in the certificate task only":
    (o, s) => {
      const found = withCommand(o, INSTALL_COMMAND.trust);
      return found.every((n) => n === model(s).trustN)
        ? null
        : `trust command in tasks [${found}]`;
    },

  "place: the proxy's install command sits in the proxy task only": (o, s) => {
    const found = withCommand(o, INSTALL_COMMAND.route);
    return found.every((n) => n === model(s).routeN)
      ? null
      : `proxy command in tasks [${found}]`;
  },

  "time: a task read done keeps only its way back, shut": (o, s) => {
    const m = model(s);
    if (m.blocked) return null;
    const done = [
      ...(m.trustDone ? [m.trustN] : []),
      ...(m.routeDone ? [m.routeN] : []),
    ].map((n) => task(o, n));
    const keeps = (t: { n: number }, g: string) =>
      WAY_BACK.has(g) ||
      (t.n === m.trustN && browsersUnprobed(s) && STEP_COUNT.test(g));
    const wrong = done.filter(
      (t) =>
        t.guides.some((g) => !keeps(t, g)) ||
        t.openGuides.some(
          (g) => g !== "details" && !(WAY_BACK.has(g) && POINTS.test(t.text)),
        ) ||
        t.commands.includes(INSTALL_COMMAND.trust) ||
        t.commands.includes(INSTALL_COMMAND.route),
    );
    return wrong.length === 0
      ? null
      : wrong
          .map((t) => `task ${t.n} is done and offers [${t.guides}]`)
          .join("; ");
  },

  "time and place: the one act exactly while both halves read not done, above the tasks it does (W12's E1)":
    (o, s) => {
      const m = model(s);
      const above = o.oneAct !== null && /^set up /.test(o.oneAct);
      const inTasks = withAction(o, "set up ");
      return above === m.oneAct && inTasks.length === 0
        ? null
        : `one act above: ${above}, in tasks [${inTasks}], expected ${m.oneAct}`;
    },

  "time: the proxy's own action exactly while it reads not done and the one act is not offered":
    (o, s) => {
      const m = model(s);
      const expected =
        m.routeHere && !m.oneAct && !m.routeDone ? [m.routeN] : [];
      const found = withAction(o, "set the system proxy");
      return same(found, expected)
        ? null
        : `set the system proxy in tasks [${found}], expected [${expected}]`;
    },

  "place and time: the reversal is in the proxy task, exactly while there is something to reverse":
    (o, s, extra) => {
      const expected = extra.reversal ? [model(s).routeN] : [];
      const found = withAction(o, REVERSAL);
      return same(found, expected)
        ? null
        : `reversal in tasks [${found}], expected [${expected}]`;
    },

  "truth: the reversal names the certificate only where it reads trusted": (
    o,
    s,
  ) => {
    const m = model(s);
    const route = o.tasks.find((t) => t.n === m.routeN);
    if (route === undefined || !route.text.includes(REVERSAL)) return null;
    const names = /the certificate stays trusted/.test(route.text);
    return names === m.trustDone ? null : `reversal line: ${route.text}`;
  },

  "truth: the end names this machine only where both probes read done": (
    o,
    s,
  ) => {
    if (o.proven === null) return null;
    const m = model(s);
    const named = o.proven.includes(`from ${m.here}`);
    const expected =
      s.platform === "this" && s.trust === "trusted" && s.proxy === "ours";
    return named === expected ? null : `the end: ${o.proven}`;
  },

  "truth: another device's end reads only what could have come from it (A4)": (
    o,
    s,
  ) => {
    const m = model(s);
    if (!(m.anotherDevice || m.sharedWire) || m.blocked) return null;
    const expected = !m.mixed && s.history === "capturing";
    return (o.proven !== null) === expected
      ? null
      : `the end: ${o.proven ?? "absent"}`;
  },

  "truth: nothing sends a trusted certificate back to its step": (o, s) => {
    const m = model(s);
    if (!m.trustDone || m.blocked) return null;
    const wrong = o.tasks.filter((t) =>
      /Finish the certificate step|The steps below install it/.test(t.text),
    );
    return wrong.length > 0
      ? `tasks [${wrong.map((t) => t.n)}] send ${s.platform} back to its certificate step`
      : null;
  },

  "truth: another device's try task keeps its proof past whatever follows it (S1)":
    (o, s) => {
      const m = model(s);
      if (!(m.anotherDevice || m.sharedWire) || m.blocked || !m.decrypted)
        return null;
      if (!m.mixed && s.history === "capturing") return null;
      const line = task(o, m.tryN).text;
      return /done — decrypted at \d\d:\d\d/.test(line) &&
        !/not working|not done/.test(line)
        ? null
        : `task ${m.tryN} reads "${line}"`;
    },

  // W6: Linux's system command reaches no browser, so advice that sends the
  // user to a browser's own store is true only where a step installs into it.
  "truth: advice names only a browser whose store a certificate step installs into":
    (o, s) => {
      const steps =
        setupFor(s.platform, {
          os: s.os,
          proxyAddr: "127.0.0.1:7420",
          reach: ANY_REACH,
        }).trust.guide?.steps ?? [];
      const stepped = steps
        .flat()
        .filter((p): p is string => typeof p === "string")
        .join("\n");
      const ownStore = o.notes.filter((note) =>
        /keeps? its own|store of (its|their) own/.test(note),
      );
      const unstepped = ["Firefox", "Chrome"].filter(
        (browser) =>
          ownStore.some((note) => note.includes(browser)) &&
          !new RegExp(`^(In|For) ${browser}`, "m").test(stepped),
      );
      return unstepped.length === 0
        ? null
        : `advice names ${unstepped.join(", ")} with no step that installs into it`;
    },

  // W7: the Simulator's wire is this machine's too, so this machine's traffic,
  // decrypted or refused, settles none of its tasks and ends nothing.
  "truth: the Simulator reads nothing unmarked (W7)": (o, s) => {
    const m = model(s);
    if (!m.sharedWire || s.marked) return null;
    const wrong = [
      ...(s.trust !== "trusted" && /^✓/.test(task(o, m.trustN).state)
        ? [`task ${m.trustN} done`]
        : []),
      ...(/done — decrypted at|capturing traffic/.test(task(o, m.tryN).text)
        ? [`task ${m.tryN} done`]
        : []),
      ...(o.proven === null ? [] : ["the end shows"]),
      ...(o.tasks.some((t) => /the Simulator does not trust/.test(t.text))
        ? ["a refusal blames the Simulator"]
        : []),
    ];
    return wrong.length === 0 ? null : wrong.join("; ");
  },

  // W8: the emulator reaches Trawler on this machine's loopback, so this
  // machine's traffic settles none of its tasks while it may be arriving, and
  // its own services' refusals never say its certificate is missing.
  "truth: the emulator reads only what could be its own, and a refusal never blames it (W8)":
    (o, s) => {
      const m = model(s);
      if (!m.emulator) return null;
      const wrong = [
        ...(/not working/.test(task(o, m.trustN).state)
          ? [`task ${m.trustN} not working`]
          : []),
        ...(o.tasks.some((t) =>
          /the emulator does not trust|Finish the certificate step/.test(
            t.text,
          ),
        )
          ? ["a refusal blames the emulator's certificate"]
          : []),
        ...(m.mixed &&
        !s.marked &&
        s.trust !== "trusted" &&
        /^✓/.test(task(o, m.trustN).state)
          ? [`task ${m.trustN} done on this machine's traffic`]
          : []),
        ...(m.mixed && !s.marked && o.proven !== null
          ? ["the end shows on this machine's traffic"]
          : []),
      ];
      return wrong.length === 0 ? null : wrong.join("; ");
    },

  // A phone's own services refuse whatever its certificate does - Apple's pin
  // theirs, Android's ignore a user certificate - so only its address refused
  // says anything of that certificate.
  "truth: a phone's certificate is not working only on its address refused, and no refusal blames it otherwise (W9, W10)":
    (o, s) => {
      const m = model(s);
      if (!m.phone || m.blocked) return null;
      const notWorking = /not working/.test(task(o, m.trustN).state);
      const blamed = o.tasks.some((t) =>
        /does not trust Trawler's certificate yet|Finish the certificate step/.test(
          t.text,
        ),
      );
      return notWorking === m.addressRefused && !blamed
        ? null
        : `task ${m.trustN} reads "${task(o, m.trustN).state}"${blamed ? "; a refusal blames the certificate" : ""}`;
    },

  "one guide open at most, beyond those a sentence points at": (o) => {
    const open = o.tasks
      .filter((t) => !POINTS.test(t.text))
      .flatMap((t) =>
        t.openGuides.filter((g) => g !== "details").map((g) => `${t.n}:${g}`),
      );
    return open.length <= 1 ? null : `open: ${open.join(", ")}`;
  },
};

// What explains a line is never on screen until asked for (W13's R1): with
// every ? shut, no caption paragraph stands outside a help panel but the step
// a dimmed task waits on. Then every ? opens, so each rule reads every word.
function explainedOnRequest(): string[] {
  const steps = screen.getByTestId("steps");
  const loud = [...steps.querySelectorAll("p.text-chrome.text-ink-mute:not(.font-mono):not(.uppercase)")]
    .filter((p) => p.closest("[data-help], [data-dim]") === null)
    .map((p) => `R1: on screen without a ? — ${p.textContent}`);
  for (const help of steps.querySelectorAll('button[aria-label^="help:"][aria-expanded="false"]'))
    fireEvent.click(help);
  return loud;
}

function judge(
  s: State,
  extra: { reversal: boolean },
  also: string[] = [],
  rules: Record<string, Rule> = RULES,
) {
  const loud = explainedOnRequest();
  const o = outline();
  const broken = Object.entries(rules)
    .map(([name, rule]) => {
      const why = rule(o, s, extra);
      return why === null ? null : `${name} — ${why}`;
    })
    .filter((line): line is string => line !== null);
  expect([...loud, ...broken, ...also]).toEqual([]);
}

const STATES: State[] = HOSTS.flatMap((os) =>
  platformChoices(os).flatMap(({ id: platform }) =>
    (OFF_MACHINE.has(platform) ? [true, false] : [false]).flatMap((armed) =>
      (platform === "this" ? [false, true] : [false]).flatMap((own) =>
        TRUST.flatMap((trust) =>
          // The trust probe reads this machine, and no off-machine task reads
          // it, so the mark is swept once against the probe that decides it.
          // The Simulator's own store is read beside its mark, so both sweep.
          (platform === "ios-sim" ||
          platform === "android-sim" ||
          (armed && trust === "unknown")
            ? [false, true]
            : [false]
          ).flatMap((marked) =>
            PROXY.flatMap((proxy) =>
              HISTORIES.flatMap((history) =>
                ((platform === "ios" || platform === "android") &&
                armed &&
                UNANSWERED.has(history)
                  ? [false, true]
                  : [false]
                ).map((address) => ({
                  ...{
                    os,
                    platform,
                    armed,
                    trust,
                    proxy,
                    history,
                    own,
                    marked,
                    address,
                  },
                })),
              ),
            ),
          ),
        ),
      ),
    ),
  ),
);

const name = (s: State) =>
  `${s.os} · ${s.platform}${OFF_MACHINE.has(s.platform) ? (s.armed ? " armed" : " unarmed") : ""}` +
  ` · trust ${s.trust} · proxy ${s.proxy} · ${s.history}${s.own ? " · own request" : ""}${s.marked ? " · marked" : ""}` +
  `${s.address ? " · its address refused" : ""}`;

describe("every reading at rest", () => {
  drainingToggles();

  test("the matrix is every host, the platforms it offers, reach, probe pair and traffic history", () => {
    const onMac = 3 + 5 * 2;
    const elsewhere = 2 + 5 * 2;
    const withOwnRequest = HOSTS.length;
    const marked =
      HOSTS.length * OFF_MACHINE.size * PROXY.length * HISTORIES.length;
    const simulatorMarked = TRUST.length * PROXY.length * HISTORIES.length;
    const emulatorMarked =
      HOSTS.length * TRUST.length * PROXY.length * HISTORIES.length;
    const phoneAddress =
      2 * HOSTS.length * (TRUST.length + 1) * PROXY.length * UNANSWERED.size;
    expect(STATES).toHaveLength(
      (onMac + 3 * elsewhere + withOwnRequest) *
        TRUST.length *
        4 *
        HISTORIES.length +
        marked +
        simulatorMarked +
        emulatorMarked +
        phoneAddress,
    );
    expect(
      STATES.filter((s) => s.platform === "ios-sim").every(
        (s) => s.os === "macos",
      ),
    ).toBe(true);
  });

  test.each(STATES.map((s) => [name(s), s] as const))("%s", (_name, s) => {
    draw(s);
    const m = model(s);
    judge(s, { reversal: m.routeHere && m.routeDone });
  });
});

// ── Every offered action, in every outcome ──────────────────────────────────

const ACTIONS = [
  { key: "setUp", label: "set up ", status: "set-up-status", routes: "set" },
  {
    key: "setProxy",
    label: "set the system proxy",
    status: "set-proxy-status",
    routes: "set",
  },
  {
    key: "unset",
    label: REVERSAL,
    status: "unset-proxy-status",
    routes: "unset",
  },
  {
    key: "trustSimulators",
    label: "trust in the Simulator",
    status: "trust-simulators-status",
    routes: null,
  },
] as const;

// One history per class of evidence - nothing, reached only, refused, decrypted,
// decrypted then refused - so every outcome meets every kind of evidence
// beside it, and a failure's note is judged next to a refusal's.
const EVIDENCE_CLASSES = new Set<History>([
  "none",
  "plain",
  "refused",
  "capturing",
  "decrypted-then-refused",
]);

const ACTED = STATES.filter(
  (s) =>
    EVIDENCE_CLASSES.has(s.history) &&
    !OFF_MACHINE.has(s.platform) &&
    (!s.own || s.history === "none"),
).flatMap((s) =>
  ACTIONS.flatMap((a) =>
    OUTCOMES.filter(
      (outcome) => a.key !== "unset" || outcome !== "nothing",
    ).map(
      (outcome) =>
        [`${name(s)} · ${a.label} → ${outcome}`, s, a, outcome] as const,
    ),
  ),
);

describe("every offered action, in every outcome", () => {
  test.each(ACTED)("%s", async (_name, s, a, outcome) => {
    draw(s, { [a.key]: outcome });
    const button = [
      ...screen.getByTestId("steps").querySelectorAll("button"),
    ].find((b) => (b.textContent ?? "").trim().startsWith(a.label));
    if (button === undefined) return;

    await act(async () => {
      fireEvent.click(button);
    });
    // The status element exists while the action runs too; the outcome is only
    // judged once it has settled, or the sweep judges the in-flight screen.
    await waitFor(() =>
      expect(
        screen.queryByTestId(a.status)?.getAttribute("data-state"),
      ).not.toBe("running"),
    );
    if (outcome !== "done" && outcome !== "nothing")
      expect(screen.getByTestId(a.status)).toHaveAttribute(
        "data-state",
        "failed",
      );

    const m = model(s);
    const setIt = a.routes === "set" && outcome === "done";
    const unsetIt = a.routes === "unset" && outcome === "done";
    const reversal = m.routeHere && !unsetIt && (m.routeDone || setIt);

    // A failure the user did not choose says "do it by hand below", so the task
    // its note sits in holds the one open guide - the one act's, the
    // certificate task under it.
    const also: string[] = [];
    if (outcome === "failed" || outcome === "unshowable") {
      const note = screen.getByTestId(a.status);
      const holder =
        note.closest('[data-testid^="task-"]') ??
        (note.closest('[data-testid="one-act"]') &&
          screen.getByTestId(`task-${m.trustN}`));
      const opened =
        holder &&
        [...holder.querySelectorAll("details[open]")].filter(topLevel);
      if (!opened || opened.length === 0)
        also.push(`${a.status} points at steps it left shut`);
    }
    judge(s, { reversal }, also);
  });
});

describe("every offered action, while it runs", () => {
  const RUNNING = STATES.filter(
    (s) =>
      EVIDENCE_CLASSES.has(s.history) &&
      !OFF_MACHINE.has(s.platform) &&
      (!s.own || s.history === "none"),
  ).flatMap((s) =>
    ACTIONS.map((a) => [`${name(s)} · ${a.label} → running`, s, a] as const),
  );

  test.each(RUNNING)("%s", async (_name, s, a) => {
    draw(s, { [a.key]: "pending" });
    const button = [
      ...screen.getByTestId("steps").querySelectorAll("button"),
    ].find((b) => (b.textContent ?? "").trim().startsWith(a.label));
    if (button === undefined) return;

    await act(async () => {
      fireEvent.click(button);
    });
    expect(screen.getByTestId(a.status)).toHaveAttribute(
      "data-state",
      "running",
    );
    judge(s, { reversal: false }, [], SAID_ONCE);
  });
});

// ── Every promise names what settles it, and that evidence settles it ──────

// A "can't check here" is a promise, so it must name something the screen can
// be shown, and showing it must move the line. This machine's promise is its
// own request in task 3 (D6); every other device's is its own first request.
function settledBy(
  s: State,
  promise: string,
): { evidence: State; offered: (o: Outline) => boolean } | null {
  const m = model(s);
  if (
    (OFF_MACHINE.has(s.platform) &&
      /^(the address in task 4|the same request) confirms it$/.test(promise)) ||
    (m.sharedWire && /^the address in task \d confirms it$/.test(promise)) ||
    (m.emulator &&
      /^(the address in task \d|the same request) confirms it$/.test(promise))
  )
    return {
      evidence: { ...s, marked: true },
      offered: (o) =>
        o.tasks.some(
          (t) => t.n === m.tryN && t.text.includes(`trawler-check=${MARK}`),
        ),
    };
  if (/^(the request in task 3|the same request) confirms it$/.test(promise))
    return {
      evidence: { ...s, own: true },
      offered: (o) =>
        o.tasks.some(
          (t) =>
            t.n === m.tryN && t.text.includes("make a request to example.com"),
        ),
    };
  if (/'s first HTTPS request confirms it$/.test(promise))
    return { evidence: { ...s, history: "capturing" }, offered: () => true };
  if (/'s first request confirms it$/.test(promise))
    return { evidence: { ...s, history: "plain" }, offered: () => true };
  return null;
}

const PROMISED = STATES.filter(
  (s) => s.history === "none" && !s.own && !s.marked && !model(s).blocked,
);

describe("every can't check here names what settles it, and that evidence moves it", () => {
  drainingToggles();

  test.each(PROMISED.map((s) => [name(s), s] as const))("%s", (_name, s) => {
    draw(s);
    const promises = outline()
      .tasks.map((t) => ({ n: t.n, line: t.state.replace(GLYPH, "").trim() }))
      .filter(({ line }) => line.startsWith("can't check here — "))
      .map(({ n, line }) => ({
        n,
        promise: line.slice("can't check here — ".length),
      }));
    const settled = promises.map(({ n, promise }) => ({
      n,
      promise,
      by: settledBy(s, promise),
    }));

    const unnamed = settled
      .filter((p) => p.by === null)
      .map((p) => `task ${p.n}: "${p.promise}"`);
    const absent = settled
      .filter((p) => p.by !== null && !p.by.offered(outline()))
      .map((p) => `task ${p.n}`);
    const unmoved: string[] = [];
    for (const { n, by } of settled) {
      if (by === null) continue;
      cleanup();
      draw(by.evidence);
      const line = task(outline(), n).state;
      if (/can't check here/.test(line))
        unmoved.push(`task ${n} still reads "${line}"`);
    }
    expect([
      ...unnamed.map((u) => `names nothing: ${u}`),
      ...absent.map((a) => `${a} names a request not offered`),
      ...unmoved,
    ]).toEqual([]);
  });
});
