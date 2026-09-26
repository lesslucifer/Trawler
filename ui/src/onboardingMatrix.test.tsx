import { describe, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { Onboarding } from "./Onboarding";
import type { OnbState } from "./onboardingMachine";
import { canAutoSetup, deviceName, platformLabel, type HostOs, type PlatformId } from "./setup";
import {
  NO_TRAFFIC,
  TUNNEL_GRACE_MS,
  type ProxyProbe,
  type Traffic,
  type TrustProbe,
} from "./verdicts";

// The state sweep of 2026-09-25, kept. Every state the setup screen can be in
// is rendered and flattened into an outline - each task's state, its actions,
// its commands and which of its guides are open, and anything drawn between
// tasks - and the outline is read against three rules rather than a snapshot:
// PLACE, an instruction sits inside the task it is for; TIME, an action or a
// step is offered only while its task is not read done; TRUTH, no sentence
// contradicts a reading on the same screen.

const HOSTS: HostOs[] = ["macos", "windows", "linux", "other"];
const PLATFORMS: PlatformId[] = ["this", "ios-sim", "android-sim"];
const TRUST: TrustProbe[] = ["trusted", "untrusted", "unknown"];
const PROXY: ProxyProbe[] = ["ours", "foreign", "none", "unknown"];
const TRAFFIC = ["waiting", "capturing", "not-decrypting"] as const;
type TrafficCase = (typeof TRAFFIC)[number];
type Outcome = "done" | "nothing" | "failed" | "refused";
const OUTCOMES: Outcome[] = ["done", "nothing", "failed", "refused"];

const onboarding: OnbState = { phase: "onboarding", step: "trust-ca", everProven: false };

const trafficOf = (reading: TrafficCase): Traffic => {
  const now = Date.now();
  if (reading === "capturing") return { ...NO_TRAFFIC, lastDecryptedAt: now - 1_000 };
  if (reading === "not-decrypting")
    return { ...NO_TRAFFIC, lastTunnelAt: now - TUNNEL_GRACE_MS - 1_000 };
  return NO_TRAFFIC;
};

const settle = (outcome: Outcome) => () => {
  if (outcome === "done") return Promise.resolve();
  if (outcome === "nothing") return Promise.resolve(null);
  return Promise.reject({ refused: outcome === "refused", detail: "scripted" });
};

interface State {
  os: HostOs;
  platform: PlatformId;
  trust: TrustProbe;
  proxy: ProxyProbe;
  traffic: TrafficCase;
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
      traffic={trafficOf(s.traffic)}
      onRecheck={vi.fn()}
      onSaveCa={vi.fn(async () => null)}
      onSetUpThisMachine={vi.fn(settle(handlers.setUp ?? "done"))}
      onSetProxy={vi.fn(settle(handlers.setProxy ?? "done"))}
      onUnsetProxy={vi.fn(settle(handlers.unset ?? "done") as () => Promise<void>)}
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

interface TaskOutline {
  n: number;
  text: string;
  actions: string[];
  commands: string[];
  openGuides: string[];
}

interface Outline {
  tasks: TaskOutline[];
  between: string[];
  proven: string | null;
  remedy: string | null;
}

const topLevel = (details: Element) => details.parentElement?.closest("details") === null;

function outline(): Outline {
  const steps = screen.getByTestId("steps");
  const children = [...steps.children];
  const isTask = (el: Element) => /^task-\d$/.test(el.getAttribute("data-testid") ?? "");
  return {
    tasks: children.filter(isTask).map((el) => ({
      n: Number(el.getAttribute("data-testid")!.slice("task-".length)),
      text: el.textContent ?? "",
      actions: [...el.querySelectorAll("button")]
        .map((b) => b.textContent?.trim() ?? "")
        .filter((label) => /^(Set up this|Set the system proxy|stop routing)/.test(label)),
      commands: [...el.querySelectorAll("button[aria-label]")].map(
        (b) => b.getAttribute("aria-label") ?? "",
      ),
      openGuides: [...el.querySelectorAll("details[open]")]
        .filter(topLevel)
        .map((d) => d.querySelector("summary")?.textContent?.replace("›", "").trim() ?? ""),
    })),
    between: children.filter((el) => !isTask(el)).map((el) => el.textContent ?? ""),
    proven: screen.queryByTestId("proven")?.textContent ?? null,
    remedy: screen.queryByTestId("traffic-remedy")?.textContent ?? null,
  };
}

// ── What the readings say ───────────────────────────────────────────────────

function readingsOf(s: State) {
  const probesTrust = s.platform === "this";
  const probesRoute = s.platform === "this" || s.platform === "ios-sim";
  const auto = canAutoSetup(s.os);
  const trustDone = probesTrust && s.trust === "trusted";
  const routeDone = probesRoute && s.proxy === "ours";
  return {
    trustDone,
    routeDone,
    routeHere: probesRoute && auto,
    oneAct: probesTrust && auto && !trustDone && !routeDone,
    here: deviceName("this", s.os),
  };
}

// Each rule answers with what it found wrong, so a failing state names the
// rule it broke and the outline that broke it.
type Rule = (o: Outline, s: State, extra: { reversal: boolean }) => string | null;

const task = (o: Outline, n: number) => o.tasks.find((t) => t.n === n)!;
const withCommand = (o: Outline, name: string) =>
  o.tasks.filter((t) => t.commands.includes(name)).map((t) => t.n);
const withAction = (o: Outline, prefix: string) =>
  o.tasks.filter((t) => t.actions.some((a) => a.startsWith(prefix))).map((t) => t.n);
const same = (a: number[], b: number[]) => a.length === b.length && a.every((x, i) => x === b[i]);

const RULES: Record<string, Rule> = {
  "place: nothing is drawn between the tasks": (o) =>
    o.between.length === 0 ? null : `between tasks: ${o.between.join(" | ")}`,

  "place: the certificate's install command sits in task 1 only": (o) =>
    withCommand(o, INSTALL_COMMAND.trust).every((n) => n === 1)
      ? null
      : `trust command in tasks ${withCommand(o, INSTALL_COMMAND.trust).join(",")}`,

  "place: the proxy's install command sits in task 2 only": (o) =>
    withCommand(o, INSTALL_COMMAND.route).every((n) => n === 2)
      ? null
      : `proxy command in tasks ${withCommand(o, INSTALL_COMMAND.route).join(",")}`,

  "time: the one act exactly while both halves read not done, in task 1": (o, s) => {
    const expected = readingsOf(s).oneAct ? [1] : [];
    const found = withAction(o, "Set up this");
    return same(found, expected) ? null : `Set up in tasks [${found}], expected [${expected}]`;
  },

  "time: the proxy's own action exactly while it reads not done and the one act is not offered": (
    o,
    s,
  ) => {
    const r = readingsOf(s);
    const expected = r.routeHere && !r.oneAct && !r.routeDone ? [2] : [];
    const found = withAction(o, "Set the system proxy");
    return same(found, expected)
      ? null
      : `Set the system proxy in tasks [${found}], expected [${expected}]`;
  },

  "time: a certificate read done offers nothing to install": (o, s) =>
    readingsOf(s).trustDone && task(o, 1).commands.includes(INSTALL_COMMAND.trust)
      ? "task 1 reads done and still carries the trust command"
      : null,

  "time: a proxy read done offers nothing to install": (o, s) =>
    readingsOf(s).routeDone && task(o, 2).commands.includes(INSTALL_COMMAND.route)
      ? "task 2 reads done and still carries the proxy command"
      : null,

  "place and time: the reversal is in task 2, exactly while there is something to reverse": (
    o,
    _s,
    extra,
  ) => {
    const expected = extra.reversal ? [2] : [];
    const found = withAction(o, REVERSAL);
    return same(found, expected) ? null : `reversal in tasks [${found}], expected [${expected}]`;
  },

  "truth: the reversal names the certificate only where it reads trusted": (o, s) => {
    const two = task(o, 2).text;
    if (!two.includes(REVERSAL)) return null;
    const names = /Trawler's certificate stays trusted/.test(two);
    return names === readingsOf(s).trustDone ? null : `reversal line: ${two}`;
  },

  "truth: the end names this machine only where both probes read done": (o, s) => {
    if (o.proven === null) return null;
    const r = readingsOf(s);
    const named = o.proven.includes(`from ${r.here}`);
    const expected = s.platform === "this" && r.trustDone && r.routeDone;
    return named === expected ? null : `the end: ${o.proven}`;
  },

  "truth: the remedy never sends a trusted certificate back to its step": (o, s) =>
    o.remedy !== null && readingsOf(s).trustDone && /Finish the certificate step/.test(o.remedy)
      ? `remedy: ${o.remedy}`
      : null,

  "one guide open at most": (o) => {
    const open = o.tasks.flatMap((t) => t.openGuides.map((g) => `${t.n}:${g}`));
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
  PLATFORMS.flatMap((platform) =>
    TRUST.flatMap((trust) =>
      PROXY.flatMap((proxy) => TRAFFIC.map((traffic) => ({ os, platform, trust, proxy, traffic }))),
    ),
  ),
);

const name = (s: State) =>
  `${s.os} · ${s.platform} · trust ${s.trust} · proxy ${s.proxy} · ${s.traffic}`;

describe("every reading at rest", () => {
  test("the matrix is the sweep's whole host-platform axis", () => {
    expect(STATES).toHaveLength(4 * 3 * 3 * 4 * 3);
  });

  test.each(STATES.map((s) => [name(s), s] as const))("%s", (_name, s) => {
    draw(s);
    const r = readingsOf(s);
    judge(s, { reversal: r.routeHere && r.routeDone });
  });
});

// ── Every offered action, in every outcome ──────────────────────────────────

const ACTIONS = [
  { key: "setUp", label: "Set up this", status: "set-up-status", sets: true },
  { key: "setProxy", label: "Set the system proxy", status: "set-proxy-status", sets: true },
  { key: "unset", label: REVERSAL, status: "unset-proxy-status", sets: false },
] as const;

const ACTED = STATES.filter((s) => s.traffic === "waiting").flatMap((s) =>
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

    const r = readingsOf(s);
    const setIt = a.sets && outcome === "done";
    const unsetIt = !a.sets && outcome === "done";
    const reversal = r.routeHere && !unsetIt && (r.routeDone || setIt);

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
