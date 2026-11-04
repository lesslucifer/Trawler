import { expect, test } from "vitest";
import {
  dotEvent,
  dotOpens,
  dotStatus,
  initial,
  opensOn,
  QUIET_AFTER_MS,
  reduce,
  stepStatus,
  type OnbState,
} from "./onboardingMachine";

const onboarding: OnbState = {
  phase: "onboarding",
  step: "trust-ca",
  everProven: false,
  since: 1_000,
  chosen: "this",
  another: false,
};

test("initial() is checking@trust-ca, never proven, opening on this machine", () => {
  expect(initial()).toEqual({
    phase: "checking",
    step: "trust-ca",
    everProven: false,
    since: null,
    chosen: "this",
    another: false,
  });
  expect(opensOn(initial())).toBe("this");
});

test("not-detected drops checking into onboarding at the first step", () => {
  expect(reduce(initial(), { t: "not-detected", at: 5_000 })).toMatchObject({
    phase: "onboarding",
    step: "trust-ca",
    since: 5_000,
  });
});

// goto is the whole navigation: the checklist is re-entered out of order from a
// step's own header, so there is no prev/next to walk or clamp.
test("goto jumps straight to a step, forwards or back", () => {
  expect(reduce(onboarding, { t: "goto", step: "verify" }).step).toBe("verify");

  const atLast = reduce(onboarding, { t: "goto", step: "verify" });
  expect(reduce(atLast, { t: "goto", step: "trust-ca" }).step).toBe("trust-ca");
});

test("captured proves capture and latches everProven", () => {
  const proven = reduce(onboarding, { t: "captured" });
  expect(proven).toMatchObject({ phase: "list", everProven: true });
});

test("re-invoke re-opens onboarding from the first step", () => {
  const proven = reduce(onboarding, { t: "captured" });
  expect(reduce(proven, { t: "re-invoke", at: 9_000 })).toMatchObject({
    phase: "onboarding",
    step: "trust-ca",
    everProven: true,
    since: 9_000,
  });
});

test("only opening setup moves since — a step, a proof and a way out keep it", () => {
  expect(reduce(onboarding, { t: "goto", step: "verify" }).since).toBe(1_000);
  const proven = reduce(onboarding, { t: "captured" });
  expect(proven.since).toBe(1_000);
  expect(reduce(reduce(proven, { t: "re-invoke", at: 2_000 }), { t: "exit" }).since).toBe(2_000);
});

// DEC-61: the list is a screen to switch to, not a reward for a proof.
test("exit opens the list from setup and from the launch check, proven or not", () => {
  expect(reduce(onboarding, { t: "exit" })).toMatchObject({ phase: "list", everProven: false });
  expect(reduce(initial(), { t: "exit" }).phase).toBe("list");
  const reopened = reduce(reduce(onboarding, { t: "captured" }), { t: "re-invoke", at: 2_000 });
  expect(reduce(reopened, { t: "exit" }).phase).toBe("list");
});

// P1: the dot is a switch in every phase, so a click from setup can never
// restart the session it is in (S2).
test("the dot opens setup from the list, and the list from setup and the launch check", () => {
  const list = reduce(onboarding, { t: "exit" });
  expect(dotOpens(list)).toBe("setup");
  expect(reduce(list, dotEvent(list, 7_000))).toMatchObject({ phase: "onboarding", since: 7_000, another: false });

  expect(dotOpens(onboarding)).toBe("list");
  expect(reduce(onboarding, dotEvent(onboarding, 7_000))).toMatchObject({ phase: "list", since: 1_000 });

  expect(dotOpens(initial())).toBe("list");
  expect(reduce(initial(), dotEvent(initial(), 7_000)).phase).toBe("list");
});

test("dotStatus is checking until the core answers, then broken with nothing captured", () => {
  expect(dotStatus(undefined, 10_000)).toBe("checking");
  expect(dotStatus(null, 10_000)).toBe("broken");
});

// P2 · Q1: a capture is proof for 3 minutes, and then the dot goes quiet.
test("dotStatus is connected for 3 minutes after the newest capture, then quiet", () => {
  const at = 1_000_000;
  expect(dotStatus(at, at)).toBe("connected");
  expect(dotStatus(at, at + QUIET_AFTER_MS - 1)).toBe("connected");
  expect(dotStatus(at, at + QUIET_AFTER_MS)).toBe("quiet");
  expect(dotStatus(at, at + 60 * QUIET_AFTER_MS)).toBe("quiet");
  expect(QUIET_AFTER_MS).toBe(3 * 60 * 1000);
});

test("choose records the platform, and every other event keeps it", () => {
  const chose = reduce(onboarding, { t: "choose", platform: "ios" });
  expect(chose.chosen).toBe("ios");
  const proven = reduce(chose, { t: "captured" });
  expect(proven.chosen).toBe("ios");
  expect(reduce(proven, { t: "re-invoke", at: 2_000, another: true }).chosen).toBe("ios");
  expect(reduce(reduce(proven, { t: "re-invoke", at: 2_000 }), { t: "exit" }).chosen).toBe("ios");
});

test("re-invoke records whether setup was opened for another device", () => {
  const proven = reduce(onboarding, { t: "captured" });
  const another = reduce(proven, { t: "re-invoke", at: 2_000, another: true });
  expect(another.another).toBe(true);
  expect(reduce(another, { t: "choose", platform: "ios" }).another).toBe(true);
  expect(reduce(reduce(another, { t: "exit" }), { t: "re-invoke", at: 3_000 }).another).toBe(false);
});

// R6 · Q2: "set up another device" never answers this machine.
test("opensOn leaves the question open only for another device over this machine", () => {
  const proven = reduce(onboarding, { t: "captured" });
  expect(opensOn(reduce(proven, { t: "re-invoke", at: 2_000, another: true }))).toBeNull();
  expect(opensOn(reduce(proven, { t: "re-invoke", at: 2_000 }))).toBe("this");

  const choseIos = reduce(reduce(onboarding, { t: "choose", platform: "ios" }), { t: "captured" });
  expect(opensOn(reduce(choseIos, { t: "re-invoke", at: 2_000, another: true }))).toBe("ios");
  expect(opensOn(reduce(choseIos, { t: "re-invoke", at: 2_000 }))).toBe("ios");
});

// E-STICKY · Q3: setup the user opens lands on the platform last named; the
// launch check's own opening stays on this machine.
test("a re-invoke carrying the remembered platform opens setup on it", () => {
  const proven = reduce(onboarding, { t: "captured" });
  expect(reduce(proven, { t: "re-invoke", at: 2_000, platform: "android" }).chosen).toBe("android");
  expect(opensOn(reduce(proven, { t: "re-invoke", at: 2_000, another: true, platform: "android" }))).toBe("android");
  expect(opensOn(reduce(proven, { t: "re-invoke", at: 2_000, another: true, platform: "this" }))).toBeNull();
  const choseIos = reduce(proven, { t: "choose", platform: "ios" });
  expect(reduce(choseIos, { t: "re-invoke", at: 3_000 }).chosen).toBe("ios");
});

test("the launch check opens setup on this machine whatever was remembered", () => {
  expect(reduce(initial(), { t: "not-detected", at: 1_000 }).chosen).toBe("this");
});

test("stepStatus derives done/active/pending from step order", () => {
  const s: OnbState = { ...onboarding, step: "set-proxy" };
  expect(stepStatus(s, "trust-ca")).toBe("done");
  expect(stepStatus(s, "set-proxy")).toBe("active");
  expect(stepStatus(s, "verify")).toBe("pending");
});
