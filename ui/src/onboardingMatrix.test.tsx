import { describe, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Onboarding } from "./Onboarding";
import type { OnbState } from "./onboardingMachine";
import type { ReachState } from "./api";
import {
  canAutoSetup,
  deviceName,
  PLATFORM_IDS,
  platformLabel,
  type HostOs,
  type PlatformId,
} from "./setup";
import {
  NO_TRAFFIC,
  RECENT_MS,
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
//   certificate  a probe that concludes (trusted / untrusted) is the reading;
//                elsewhere the traffic is - a decrypted request is done, a
//                refusal with none decrypted is not done, and nothing yet is a
//                promise to confirm on the first HTTPS request
//   proxy        where a probe exists (this machine, the Simulator) it is the
//                reading; elsewhere any request that reached Trawler is done
//
// and against three kinds of rule. PLACE, an instruction sits inside the task
// it is for; TIME, an action or a step is offered only while its task is not
// read done; TRUTH, no sentence contradicts a reading on the same screen.

const HOSTS: HostOs[] = ["macos", "windows", "linux", "other"];
const TRUST: TrustProbe[] = ["trusted", "untrusted", "unknown"];
const PROXY: ProxyProbe[] = ["ours", "foreign", "none", "unknown"];

// Traffic is a history, not a moment: the eight shapes the evidence can take.
const HISTORIES = [
  "none",
  "plain",
  "tunnel-in-grace",
  "refused",
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
      return { ...NO_TRAFFIC, lastTunnelAt: ago(0) };
    case "refused":
      return { ...NO_TRAFFIC, lastTunnelAt: ago(PAST_GRACE) };
    case "capturing":
      return { ...NO_TRAFFIC, lastDecryptedAt: ago(1_000) };
    case "decrypted-earlier":
      return { ...NO_TRAFFIC, lastDecryptedAt: ago(EARLIER) };
    case "refused-earlier":
      return { ...NO_TRAFFIC, lastTunnelAt: ago(EARLIER) };
    case "decrypted-then-refused":
      return { ...NO_TRAFFIC, lastDecryptedAt: ago(EARLIER), lastTunnelAt: ago(PAST_GRACE) };
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

type Outcome = "done" | "nothing" | "failed" | "refused";
const OUTCOMES: Outcome[] = ["done", "nothing", "failed", "refused"];

const onboarding: OnbState = { phase: "onboarding", step: "trust-ca", everProven: false, since: null };

const settle = (outcome: Outcome) => () => {
  if (outcome === "done") return Promise.resolve();
  if (outcome === "nothing") return Promise.resolve(null);
  return Promise.reject({ refused: outcome === "refused", detail: "scripted" });
};

interface State {
  os: HostOs;
  platform: PlatformId;
  armed: boolean;
  trust: TrustProbe;
  proxy: ProxyProbe;
  history: History;
}

type Handlers = Partial<Record<"setUp" | "setProxy" | "unset", Outcome>>;

function draw(s: State, handlers: Handlers = {}) {
  render(
    <Onboarding
      state={onboarding}
      dispatch={vi.fn()}
      os={s.os}
      proxyAddr="127.0.0.1:9999"
      trustProbe={s.trust}
      proxy={{ status: s.proxy, address: s.proxy === "foreign" ? "10.1.1.1:8080" : null }}
      traffic={trafficOf(s.history)}
      reach={s.armed ? ARMED : UNARMED}
      onRecheck={vi.fn()}
      onSaveCa={vi.fn(async () => null)}
      onSetUpThisMachine={vi.fn(settle(handlers.setUp ?? "done"))}
      onSetProxy={vi.fn(settle(handlers.setProxy ?? "done"))}
      onUnsetProxy={vi.fn(settle(handlers.unset ?? "done") as () => Promise<void>)}
      onArmReach={vi.fn(async () => {})}
      onDisarmReach={vi.fn(async () => {})}
    />,
  );
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
}

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
  };
}

// ── The model ───────────────────────────────────────────────────────────────

function model(s: State) {
  const probesTrust = s.platform === "this";
  const probesRoute = s.platform === "this" || s.platform === "ios-sim";
  const needsReach = OFF_MACHINE.has(s.platform);
  const first = needsReach ? 1 : 0;
  const h = s.history;
  const reached = h !== "none";
  const decrypted = h === "capturing" || h === "decrypted-earlier" || h === "decrypted-then-refused";
  const unanswered = h === "refused" || h === "refused-earlier" || h === "decrypted-then-refused";
  const refused = unanswered && !decrypted;
  const trustConcluded = probesTrust && s.trust !== "unknown";
  const trustDone = trustConcluded ? s.trust === "trusted" : decrypted;
  const routeDone = probesRoute ? s.proxy === "ours" : reached;
  const auto = canAutoSetup(s.os);
  const routeHere = probesRoute && auto;
  const oneAct = probesTrust && auto && !trustDone && !routeDone;
  // What the try task's clock says now: a refusal inside the window is "not
  // decrypting"; one before it is quiet.
  const notDecryptingNow = h === "refused" || h === "decrypted-then-refused";
  return {
    ...{ probesTrust, probesRoute, first, reached, decrypted, unanswered, refused },
    trustN: first + 1,
    routeN: first + 2,
    tryN: first + 3,
    blocked: needsReach && !s.armed,
    trustConcluded,
    trustDone,
    routeDone,
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
  if (m.trustConcluded)
    return s.trust === "trusted"
      ? /^✓\s*done — installed and trusted$/
      : /^○\s*not done — the certificate isn't trusted$/;
  if (m.decrypted) return /^✓\s*done — a request decrypted$/;
  if (m.refused) return /^⚠\s*not done — nothing decrypted$/;
  return /^◐\s*we'll confirm this on .+'s first HTTPS request$/;
}

function expectedRouteState(s: State): RegExp {
  const m = model(s);
  if (!m.probesRoute)
    return m.reached
      ? /^✓\s*done — requests reached Trawler$/
      : /^◐\s*we'll confirm this on .+'s first request$/;
  if (s.proxy === "ours") return /^✓\s*done — .+ routes through Trawler$/;
  if (s.proxy === "foreign") return /^○\s*not done — another proxy is set$/;
  if (s.proxy === "none") return /^○\s*not done — the system proxy is off$/;
  return /^◐\s*we'll confirm this on .+'s first request$/;
}

const RULES: Record<string, Rule> = {
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

  "place: a refusal is said once - in the certificate task while it is not done, else in the try task while nothing decrypts now":
    (o, s) => {
      const m = model(s);
      const expected = m.blocked
        ? []
        : !m.trustDone && m.unanswered
          ? [m.trustN]
          : m.trustDone && m.notDecryptingNow
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

  "truth: nothing sends a trusted certificate back to its step": (o, s) => {
    const m = model(s);
    if (!m.trustDone) return null;
    const wrong = o.tasks.filter((t) => /Finish the certificate step|The steps below install it/.test(t.text));
    return s.platform === "this" && wrong.length > 0
      ? `tasks [${wrong.map((t) => t.n)}] send this machine back to its certificate step`
      : null;
  },

  "one guide open at most, beyond those a sentence points at": (o) => {
    const open = o.tasks
      .filter((t) => !/the steps below/i.test(t.text))
      .flatMap((t) => t.openGuides.filter((g) => g !== "details").map((g) => `${t.n}:${g}`));
    return open.length <= 1 ? null : `open: ${open.join(", ")}`;
  },
};

function judge(s: State, extra: { reversal: boolean }, also: string[] = []) {
  const o = outline();
  const broken = Object.entries(RULES)
    .map(([name, rule]) => {
      const why = rule(o, s, extra);
      return why === null ? null : `${name} — ${why}`;
    })
    .filter((line): line is string => line !== null);
  expect([...broken, ...also]).toEqual([]);
}

const STATES: State[] = HOSTS.flatMap((os) =>
  PLATFORM_IDS.flatMap((platform) =>
    (OFF_MACHINE.has(platform) ? [true, false] : [false]).flatMap((armed) =>
      TRUST.flatMap((trust) =>
        PROXY.flatMap((proxy) =>
          HISTORIES.map((history) => ({ os, platform, armed, trust, proxy, history })),
        ),
      ),
    ),
  ),
);

const name = (s: State) =>
  `${s.os} · ${s.platform}${OFF_MACHINE.has(s.platform) ? (s.armed ? " armed" : " unarmed") : ""}` +
  ` · trust ${s.trust} · proxy ${s.proxy} · ${s.history}`;

describe("every reading at rest", () => {
  test("the matrix is every host, platform, reach, probe pair and traffic history", () => {
    expect(STATES).toHaveLength(4 * (3 + 5 * 2) * 3 * 4 * HISTORIES.length);
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
  (s) => EVIDENCE_CLASSES.has(s.history) && !OFF_MACHINE.has(s.platform),
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
    if (outcome === "failed") {
      const note = screen.getByTestId(a.status);
      const holder = note.closest('[data-testid^="task-"]');
      const opened = holder && [...holder.querySelectorAll("details[open]")].filter(topLevel);
      if (!opened || opened.length === 0) also.push(`${a.status} points at steps it left shut`);
    }
    judge(s, { reversal }, also);
  });
});
