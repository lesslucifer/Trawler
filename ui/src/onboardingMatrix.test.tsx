import { afterEach, describe, expect, test } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Onboarding, useRouteMemory } from "./Onboarding";
import type { OnbState } from "./onboardingMachine";
import type { ReachState } from "./api";
import {
  canAutoSetup,
  deviceName,
  platformChoices,
  platformLabel,
  type HostOs,
  type PlatformId,
} from "./setup";
import {
  mayCarryOwnTraffic,
  NO_TRAFFIC,
  RECENT_MS,
  STATE_WORDS,
  TUNNEL_GRACE_MS,
  type ProxyProbe,
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
//
// and against three kinds of rule. PLACE, an instruction sits inside the task
// it is for; TIME, an action or a step is offered only while its task is not
// read done; TRUTH, no sentence contradicts a reading on the same screen.

// jsdom fires a <details> element's toggle from a timer it queues when the
// element opens, and the timer holds the element's whole rendered tree until it
// runs. A synchronous test never yields to it, so a sweep of them keeps every
// open guide it drew, and outgrows a worker's heap unless each test drains it.
const drainingToggles = () => afterEach(() => new Promise((resolve) => setTimeout(resolve, 0)));

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
      return { ...NO_TRAFFIC, lastTunnelAt: ago(PAST_GRACE), openTunnelAt: ago(PAST_GRACE) };
    case "refused-and-retrying":
      return { ...NO_TRAFFIC, lastTunnelAt: ago(0), openTunnelAt: ago(PAST_GRACE) };
    case "capturing":
      return { ...NO_TRAFFIC, lastDecryptedAt: ago(1_000) };
    case "decrypted-earlier":
      return { ...NO_TRAFFIC, lastDecryptedAt: ago(EARLIER) };
    case "refused-earlier":
      return { ...NO_TRAFFIC, lastTunnelAt: ago(EARLIER), openTunnelAt: ago(EARLIER) };
    case "decrypted-then-refused":
      return {
        ...NO_TRAFFIC,
        lastDecryptedAt: ago(EARLIER),
        lastTunnelAt: ago(PAST_GRACE),
        openTunnelAt: ago(PAST_GRACE),
      };
  }
}

const OFF_MACHINE = new Set<PlatformId>(["mac", "windows", "ubuntu", "ios", "android"]);

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
const UNARMED: ReachState = { ...ARMED, armed: false, proxy: null, api: null, ca_urls: null };

type Outcome = "done" | "nothing" | "failed" | "refused" | "unshowable" | "pending";
const OUTCOMES: Outcome[] = ["done", "nothing", "failed", "refused", "unshowable"];

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
  const lastDecryptedAt = Math.max(traffic.lastDecryptedAt ?? markedAt, markedAt);
  const openTunnelAt =
    traffic.openTunnelAt !== null && traffic.openTunnelAt > lastDecryptedAt ? traffic.openTunnelAt : null;
  return { traffic: { ...traffic, lastDecryptedAt, openTunnelAt }, markedAt };
}

type Handlers = Partial<Record<"setUp" | "setProxy" | "unset", Outcome>>;

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
  const mixed = OFF_MACHINE.has(s.platform) && mayCarryOwnTraffic(s.proxy);
  const its = !mixed ? traffic : markedAt === null ? NO_TRAFFIC : { ...NO_TRAFFIC, lastDecryptedAt: markedAt };
  return (
    <Onboarding
      state={onboarding}
      dispatch={nothing}
      os={s.os}
      proxyAddr="127.0.0.1:9999"
      trustProbe={s.trust}
      proxy={{ status: s.proxy, address: s.proxy === "foreign" ? "10.1.1.1:8080" : null }}
      traffic={its}
      ownProofAt={s.own ? OWN_AT : null}
      deviceMark={MARK}
      device={{ provedAt: markedAt, mixed, wire: traffic }}
      reach={s.armed ? ARMED : UNARMED}
      onRecheck={nothing}
      onSaveCa={nothingSaved}
      onSetUpThisMachine={settle(handlers.setUp ?? "done")}
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
    fireEvent.click(screen.getByRole("option", { name: platformLabel(s.platform, s.os) }));
  }
}

// ── The outline ─────────────────────────────────────────────────────────────

const INSTALL_COMMAND = { trust: "the trust command", route: "the proxy command" } as const;
const REVERSAL = "stop routing through Trawler";
const WAY_BACK = new Set(["to remove it later", "to turn it off later", "details"]);

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
  between: string[];
  proven: string | null;
  lines: string[];
  notes: string[];
}

const GLYPH = /^[✓○⚠◐⟳]?\s*/;
const textOf = (el: Element) => (el.textContent ?? "").replace(GLYPH, "").trim();

const topLevel = (details: Element) => details.parentElement?.closest("details") === null;
const summaryOf = (d: Element) =>
  d.querySelector("summary")?.textContent?.replace("›", "").trim() ?? "";

function outline(): Outline {
  const steps = screen.getByTestId("steps");
  const children = [...steps.children];
  const isTask = (el: Element) => /^task-\d$/.test(el.getAttribute("data-testid") ?? "");
  return {
    tasks: children.filter(isTask).map((el) => {
      const n = Number(el.getAttribute("data-testid")!.slice("task-".length));
      const guides = [...el.querySelectorAll("details")].filter(topLevel);
      return {
        n,
        state: el.querySelector(`[data-testid="task-${n}-state"]`)?.textContent ?? "",
        text: el.textContent ?? "",
        actions: [...el.querySelectorAll("button")]
          .map((b) => b.textContent?.trim() ?? "")
          .filter((label) => /^(Set up this|Set the system proxy|stop routing)/.test(label)),
        commands: [...el.querySelectorAll("button[aria-label]")].map(
          (b) => b.getAttribute("aria-label") ?? "",
        ),
        guides: guides.map(summaryOf),
        openGuides: guides.filter((d) => d.hasAttribute("open")).map(summaryOf),
        refusals: [...el.querySelectorAll('[data-testid="traffic-remedy"]')].filter((r) =>
          /decrypt/.test(r.textContent ?? ""),
        ).length,
      };
    }),
    between: children.filter((el) => !isTask(el)).map((el) => el.textContent ?? ""),
    proven: screen.queryByTestId("proven")?.textContent ?? null,
    lines: [...steps.querySelectorAll('[data-testid$="-state"]')].map(textOf),
    notes: [
      ...steps.querySelectorAll('[data-testid$="-status"] > :first-child, [data-testid="traffic-remedy"]'),
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
    decrypted: s.marked || h === "capturing" || h === "decrypted-earlier" || h === "decrypted-then-refused",
    unanswered:
      h === "refused" ||
      h === "refused-and-retrying" ||
      h === "decrypted-then-refused" ||
      (h === "refused-earlier" && !s.marked),
  };
  // DEC-63: where this machine's own traffic may be arriving too, only the
  // marked request is another device's evidence.
  const mixed = needsReach && (s.proxy === "ours" || s.proxy === "unknown");
  const reached = mixed ? s.marked : onWire.reached;
  const decrypted = mixed ? s.marked : onWire.decrypted;
  const unanswered = mixed ? false : onWire.unanswered;
  const refused = unanswered && !decrypted;
  const trustConcluded = probesTrust && s.trust !== "unknown";
  const trustDone = !probesTrust ? decrypted : trustConcluded ? s.trust === "trusted" : s.own;
  const routeDone =
    probesRoute && s.proxy !== "unknown" ? s.proxy === "ours" : s.platform === "this" ? s.own : reached;
  // A refusal is the certificate task's while it is not done - except where
  // the refusal may be anyone's: This machine's store unread (D7), another
  // device's traffic mixed with this machine's (A3).
  const certificateOwnsRefusal = !trustDone && !(probesTrust && !trustConcluded) && !mixed;
  const auto = canAutoSetup(s.os);
  const routeHere = probesRoute && auto;
  const oneAct = probesTrust && auto && !trustDone && !routeDone;
  // What the try task's clock says now: a refusal inside the window is "not
  // decrypting", however new the tunnel after it; one before it is quiet.
  const notDecryptingNow = h === "refused" || h === "refused-and-retrying" || h === "decrypted-then-refused";
  return {
    ...{ probesTrust, probesRoute, first, reached, decrypted, unanswered, refused, mixed },
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
    notDecryptingNow,
    here: deviceName("this", s.os),
  };
}

type Rule = (o: Outline, s: State, extra: { reversal: boolean }) => string | null;

const task = (o: Outline, n: number) => o.tasks.find((t) => t.n === n)!;
const withCommand = (o: Outline, name: string) =>
  o.tasks.filter((t) => t.commands.includes(name)).map((t) => t.n);
const withAction = (o: Outline, prefix: string) =>
  o.tasks.filter((t) => t.actions.some((a) => a.startsWith(prefix))).map((t) => t.n);
const same = (a: number[], b: number[]) => a.length === b.length && a.every((x, i) => x === b[i]);

function expectedTrustState(s: State): RegExp {
  const m = model(s);
  if (m.probesTrust && !m.trustConcluded)
    return s.own
      ? /^✓\s*done — step 3's request decrypted at \d\d:\d\d$/
      : /^◐\s*can't check here — the request in step 3 confirms it$/;
  if (m.trustConcluded)
    return {
      trusted: /^✓\s*done — installed and trusted$/,
      installed: /^○\s*not done — installed, not trusted$/,
      absent: /^○\s*not done — not installed$/,
    }[s.trust as "trusted" | "installed" | "absent"];
  if (s.marked) return /^✓\s*done — .+'s request decrypted at \d\d:\d\d$/;
  if (m.mixed) return /^◐\s*can't check here — the address in step 4 confirms it$/;
  if (m.decrypted && OFF_MACHINE.has(s.platform)) return /^✓\s*done — a request decrypted at \d\d:\d\d$/;
  if (m.decrypted) return /^✓\s*done — a request decrypted$/;
  if (m.refused) return /^⚠\s*not working — nothing decrypted$/;
  return /^◐\s*can't check here — .+'s first HTTPS request confirms it$/;
}

function expectedRouteState(s: State): RegExp {
  const m = model(s);
  if (s.marked) return /^✓\s*done — .+'s request came through at \d\d:\d\d$/;
  if (m.mixed) return /^◐\s*can't check here — the same request confirms it$/;
  if (m.reached && OFF_MACHINE.has(s.platform)) return /^✓\s*done — a request reached Trawler at \d\d:\d\d$/;
  if (!m.probesRoute || (s.proxy === "unknown" && s.platform !== "this"))
    return m.reached
      ? /^✓\s*done — requests reached Trawler$/
      : /^◐\s*can't check here — .+'s first request confirms it$/;
  if (s.proxy === "ours") return /^✓\s*done — .+ routes through Trawler$/;
  if (s.proxy === "foreign") return /^○\s*not done — another proxy is set$/;
  if (s.proxy === "none") return /^○\s*not done — the system proxy is off$/;
  if (s.platform === "this")
    return s.own
      ? /^✓\s*done — step 3's request came through at \d\d:\d\d$/
      : /^◐\s*can't check here — the same request confirms it$/;
  return /^◐\s*can't check here — .+'s first request confirms it$/;
}

// F19: a state line's reason and a note are each a sentence, and a sentence is
// said once. A bare state word is a state, not a sentence — two tasks a write
// covers may both read "setting up".
function repeated(o: Outline): string[] {
  const reasons = o.lines.map((line) => line.split(" — ").slice(1).join(" — ")).filter(Boolean);
  const sentences = [...reasons, ...o.notes, ...o.notes.flatMap((n) => (o.lines.includes(n) ? [n] : []))];
  return [...new Set(sentences.filter((x, i) => sentences.indexOf(x) !== i))];
}

const SAID_ONCE: Record<string, Rule> = {
  "truth: every state line opens with a state word (F5)": (o) => {
    const wordless = o.lines.filter(
      (line) => !STATE_WORDS.some((w) => line === w || line.startsWith(`${w} — `)),
    );
    return wordless.length === 0 ? null : `lines without a state word: ${wordless.join(" | ")}`;
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
    return expectedTrustState(s).test(line) ? null : `task ${m.trustN} reads "${line}"`;
  },

  "truth: the proxy task reads what its reading says": (o, s) => {
    const m = model(s);
    if (m.blocked) return null;
    const line = task(o, m.routeN).state;
    return expectedRouteState(s).test(line) ? null : `task ${m.routeN} reads "${line}"`;
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
      const found = o.tasks.flatMap((t) => Array(t.refusals).fill(t.n) as number[]);
      return same(found, expected) ? null : `refusal in tasks [${found}], expected [${expected}]`;
    },

  "truth: every sentence that points at the steps below has them open": (o) => {
    const pointing = o.tasks.filter((t) => /the steps below/i.test(t.text));
    const shut = pointing.filter((t) => t.openGuides.length === 0).map((t) => t.n);
    return shut.length === 0 ? null : `tasks [${shut}] point at steps left shut`;
  },

  "place: nothing is drawn between the tasks": (o) =>
    o.between.length === 0 ? null : `between tasks: ${o.between.join(" | ")}`,

  "place: the certificate's install command sits in the certificate task only": (o, s) => {
    const found = withCommand(o, INSTALL_COMMAND.trust);
    return found.every((n) => n === model(s).trustN) ? null : `trust command in tasks [${found}]`;
  },

  "place: the proxy's install command sits in the proxy task only": (o, s) => {
    const found = withCommand(o, INSTALL_COMMAND.route);
    return found.every((n) => n === model(s).routeN) ? null : `proxy command in tasks [${found}]`;
  },

  "time: a task read done keeps only its way back, shut": (o, s) => {
    const m = model(s);
    if (m.blocked) return null;
    const done = [
      ...(m.trustDone ? [m.trustN] : []),
      ...(m.routeDone ? [m.routeN] : []),
    ].map((n) => task(o, n));
    const wrong = done.filter(
      (t) =>
        t.guides.some((g) => !WAY_BACK.has(g)) ||
        t.openGuides.some(
          (g) => g !== "details" && !(WAY_BACK.has(g) && /the steps below/i.test(t.text)),
        ) ||
        t.commands.includes(INSTALL_COMMAND.trust) ||
        t.commands.includes(INSTALL_COMMAND.route),
    );
    return wrong.length === 0
      ? null
      : wrong.map((t) => `task ${t.n} is done and offers [${t.guides}]`).join("; ");
  },

  "time: the one act exactly while both halves read not done, in the certificate task": (o, s) => {
    const m = model(s);
    const expected = m.oneAct ? [m.trustN] : [];
    const found = withAction(o, "Set up this");
    return same(found, expected) ? null : `Set up in tasks [${found}], expected [${expected}]`;
  },

  "time: the proxy's own action exactly while it reads not done and the one act is not offered": (
    o,
    s,
  ) => {
    const m = model(s);
    const expected = m.routeHere && !m.oneAct && !m.routeDone ? [m.routeN] : [];
    const found = withAction(o, "Set the system proxy");
    return same(found, expected)
      ? null
      : `Set the system proxy in tasks [${found}], expected [${expected}]`;
  },

  "place and time: the reversal is in the proxy task, exactly while there is something to reverse":
    (o, s, extra) => {
      const expected = extra.reversal ? [model(s).routeN] : [];
      const found = withAction(o, REVERSAL);
      return same(found, expected) ? null : `reversal in tasks [${found}], expected [${expected}]`;
    },

  "truth: the reversal names the certificate only where it reads trusted": (o, s) => {
    const m = model(s);
    const route = o.tasks.find((t) => t.n === m.routeN);
    if (route === undefined || !route.text.includes(REVERSAL)) return null;
    const names = /Trawler's certificate stays trusted/.test(route.text);
    return names === m.trustDone ? null : `reversal line: ${route.text}`;
  },

  "truth: the end names this machine only where both probes read done": (o, s) => {
    if (o.proven === null) return null;
    const m = model(s);
    const named = o.proven.includes(`from ${m.here}`);
    const expected = s.platform === "this" && s.trust === "trusted" && s.proxy === "ours";
    return named === expected ? null : `the end: ${o.proven}`;
  },

  "truth: another device's end reads only what could have come from it (A4)": (o, s) => {
    const m = model(s);
    if (!OFF_MACHINE.has(s.platform) || m.blocked) return null;
    const expected = !m.mixed && s.history === "capturing";
    return (o.proven !== null) === expected ? null : `the end: ${o.proven ?? "absent"}`;
  },

  "truth: nothing sends a trusted certificate back to its step": (o, s) => {
    const m = model(s);
    if (!m.trustDone || m.blocked) return null;
    const wrong = o.tasks.filter((t) => /Finish the certificate step|The steps below install it/.test(t.text));
    return (s.platform === "this" || OFF_MACHINE.has(s.platform)) && wrong.length > 0
      ? `tasks [${wrong.map((t) => t.n)}] send ${s.platform} back to its certificate step`
      : null;
  },

  "truth: another device's try task keeps its proof past whatever follows it (S1)": (o, s) => {
    const m = model(s);
    if (!OFF_MACHINE.has(s.platform) || m.blocked || !m.decrypted) return null;
    if (!m.mixed && s.history === "capturing") return null;
    const line = task(o, m.tryN).text;
    return /done — decrypted at \d\d:\d\d/.test(line) && !/not working|not done/.test(line)
      ? null
      : `task ${m.tryN} reads "${line}"`;
  },

  "one guide open at most, beyond those a sentence points at": (o) => {
    const open = o.tasks
      .filter((t) => !/the steps below/i.test(t.text))
      .flatMap((t) => t.openGuides.filter((g) => g !== "details").map((g) => `${t.n}:${g}`));
    return open.length <= 1 ? null : `open: ${open.join(", ")}`;
  },
};

function judge(
  s: State,
  extra: { reversal: boolean },
  also: string[] = [],
  rules: Record<string, Rule> = RULES,
) {
  const o = outline();
  const broken = Object.entries(rules)
    .map(([name, rule]) => {
      const why = rule(o, s, extra);
      return why === null ? null : `${name} — ${why}`;
    })
    .filter((line): line is string => line !== null);
  expect([...broken, ...also]).toEqual([]);
}

const STATES: State[] = HOSTS.flatMap((os) =>
  platformChoices(os).flatMap(({ id: platform }) =>
    (OFF_MACHINE.has(platform) ? [true, false] : [false]).flatMap((armed) =>
      (platform === "this" ? [false, true] : [false]).flatMap((own) =>
        TRUST.flatMap((trust) =>
          // The trust probe reads this machine, and no off-machine task reads
          // it, so the mark is swept once against the probe that decides it.
          (armed && trust === "unknown" ? [false, true] : [false]).flatMap((marked) =>
            PROXY.flatMap((proxy) =>
              HISTORIES.map((history) => ({ os, platform, armed, trust, proxy, history, own, marked })),
            ),
          ),
        ),
      ),
    ),
  ),
);

const name = (s: State) =>
  `${s.os} · ${s.platform}${OFF_MACHINE.has(s.platform) ? (s.armed ? " armed" : " unarmed") : ""}` +
  ` · trust ${s.trust} · proxy ${s.proxy} · ${s.history}${s.own ? " · own request" : ""}${s.marked ? " · marked" : ""}`;

describe("every reading at rest", () => {
  drainingToggles();

  test("the matrix is every host, the platforms it offers, reach, probe pair and traffic history", () => {
    const onMac = 3 + 5 * 2;
    const elsewhere = 2 + 5 * 2;
    const withOwnRequest = HOSTS.length;
    const marked = HOSTS.length * OFF_MACHINE.size * PROXY.length * HISTORIES.length;
    expect(STATES).toHaveLength(
      (onMac + 3 * elsewhere + withOwnRequest) * TRUST.length * 4 * HISTORIES.length + marked,
    );
    expect(STATES.filter((s) => s.platform === "ios-sim").every((s) => s.os === "macos")).toBe(true);
  });

  test.each(STATES.map((s) => [name(s), s] as const))("%s", (_name, s) => {
    draw(s);
    const m = model(s);
    judge(s, { reversal: m.routeHere && m.routeDone });
  });
});

// ── Every offered action, in every outcome ──────────────────────────────────

const ACTIONS = [
  { key: "setUp", label: "Set up this", status: "set-up-status", sets: true },
  { key: "setProxy", label: "Set the system proxy", status: "set-proxy-status", sets: true },
  { key: "unset", label: REVERSAL, status: "unset-proxy-status", sets: false },
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
  (s) => EVIDENCE_CLASSES.has(s.history) && !OFF_MACHINE.has(s.platform) && (!s.own || s.history === "none"),
).flatMap(
  (s) =>
    ACTIONS.flatMap((a) =>
      OUTCOMES.filter((outcome) => a.key !== "unset" || outcome !== "nothing").map(
        (outcome) => [`${name(s)} · ${a.label} → ${outcome}`, s, a, outcome] as const,
      ),
    ),
);

describe("every offered action, in every outcome", () => {
  test.each(ACTED)("%s", async (_name, s, a, outcome) => {
    draw(s, { [a.key]: outcome });
    const button = [...screen.getByTestId("steps").querySelectorAll("button")].find((b) =>
      (b.textContent ?? "").trim().startsWith(a.label),
    );
    if (button === undefined) return;

    await act(async () => {
      fireEvent.click(button);
    });
    // The status element exists while the action runs too; the outcome is only
    // judged once it has settled, or the sweep judges the in-flight screen.
    await waitFor(() =>
      expect(screen.queryByTestId(a.status)?.getAttribute("data-state")).not.toBe("running"),
    );
    if (outcome !== "done" && outcome !== "nothing")
      expect(screen.getByTestId(a.status)).toHaveAttribute("data-state", "failed");

    const m = model(s);
    const setIt = a.sets && outcome === "done";
    const unsetIt = !a.sets && outcome === "done";
    const reversal = m.routeHere && !unsetIt && (m.routeDone || setIt);

    // A failure the user did not choose says "the steps below do it by hand",
    // so the task its note sits in holds the one open guide.
    const also: string[] = [];
    if (outcome === "failed" || outcome === "unshowable") {
      const note = screen.getByTestId(a.status);
      const holder = note.closest('[data-testid^="task-"]');
      const opened = holder && [...holder.querySelectorAll("details[open]")].filter(topLevel);
      if (!opened || opened.length === 0) also.push(`${a.status} points at steps it left shut`);
    }
    judge(s, { reversal }, also);
  });
});

describe("every offered action, while it runs", () => {
  const RUNNING = STATES.filter(
    (s) => EVIDENCE_CLASSES.has(s.history) && !OFF_MACHINE.has(s.platform) && (!s.own || s.history === "none"),
  ).flatMap((s) => ACTIONS.map((a) => [`${name(s)} · ${a.label} → running`, s, a] as const));

  test.each(RUNNING)("%s", async (_name, s, a) => {
    draw(s, { [a.key]: "pending" });
    const button = [...screen.getByTestId("steps").querySelectorAll("button")].find((b) =>
      (b.textContent ?? "").trim().startsWith(a.label),
    );
    if (button === undefined) return;

    await act(async () => {
      fireEvent.click(button);
    });
    expect(screen.getByTestId(a.status)).toHaveAttribute("data-state", "running");
    judge(s, { reversal: false }, [], SAID_ONCE);
  });
});

// ── Every promise names what settles it, and that evidence settles it ──────

// A "can't check here" is a promise, so it must name something the screen can
// be shown, and showing it must move the line. This machine's promise is its
// own request in step 3 (D6); every other device's is its own first request.
function settledBy(s: State, promise: string): { evidence: State; offered: (o: Outline) => boolean } | null {
  const m = model(s);
  if (OFF_MACHINE.has(s.platform) && /^(the address in step 4|the same request) confirms it$/.test(promise))
    return {
      evidence: { ...s, marked: true },
      offered: (o) => o.tasks.some((t) => t.n === m.tryN && t.text.includes(`trawler-check=${MARK}`)),
    };
  if (/^(the request in step 3|the same request) confirms it$/.test(promise))
    return {
      evidence: { ...s, own: true },
      offered: (o) => o.tasks.some((t) => t.n === m.tryN && t.text.includes("make a request to example.com")),
    };
  if (/'s first HTTPS request confirms it$/.test(promise))
    return { evidence: { ...s, history: "capturing" }, offered: () => true };
  if (/'s first request confirms it$/.test(promise))
    return { evidence: { ...s, history: "plain" }, offered: () => true };
  return null;
}

const PROMISED = STATES.filter((s) => s.history === "none" && !s.own && !s.marked && !model(s).blocked);

describe("every can't check here names what settles it, and that evidence moves it", () => {
  drainingToggles();

  test.each(PROMISED.map((s) => [name(s), s] as const))("%s", (_name, s) => {
    draw(s);
    const promises = outline()
      .tasks.map((t) => ({ n: t.n, line: t.state.replace(GLYPH, "").trim() }))
      .filter(({ line }) => line.startsWith("can't check here — "))
      .map(({ n, line }) => ({ n, promise: line.slice("can't check here — ".length) }));
    const settled = promises.map(({ n, promise }) => ({ n, promise, by: settledBy(s, promise) }));

    const unnamed = settled.filter((p) => p.by === null).map((p) => `task ${p.n}: "${p.promise}"`);
    const absent = settled.filter((p) => p.by !== null && !p.by.offered(outline())).map((p) => `task ${p.n}`);
    const unmoved: string[] = [];
    for (const { n, by } of settled) {
      if (by === null) continue;
      cleanup();
      draw(by.evidence);
      const line = task(outline(), n).state;
      if (/can't check here/.test(line)) unmoved.push(`task ${n} still reads "${line}"`);
    }
    expect([...unnamed.map((u) => `names nothing: ${u}`), ...absent.map((a) => `${a} names a request not offered`), ...unmoved]).toEqual([]);
  });
});
