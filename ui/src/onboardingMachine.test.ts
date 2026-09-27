import { expect, test } from "vitest";
import { dotStatus, initial, opensOn, reduce, stepStatus, type OnbState } from "./onboardingMachine";

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
  expect(proven).toMatchObject({ phase: "proven", everProven: true });
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

test("exit returns to proven only once ever proven, else no-op", () => {
  expect(reduce(onboarding, { t: "exit" })).toBe(onboarding); // never proven → no-op
  const reopened = reduce(reduce(onboarding, { t: "captured" }), { t: "re-invoke", at: 2_000 });
  expect(reduce(reopened, { t: "exit" }).phase).toBe("proven");
});

test("dotStatus maps each phase", () => {
  expect(dotStatus({ ...onboarding, phase: "checking" })).toBe("checking");
  expect(dotStatus({ ...onboarding, phase: "onboarding" })).toBe("broken");
  expect(dotStatus({ ...onboarding, phase: "proven" })).toBe("connected");
});

// R5: setting up a second device is not capture breaking on the first.
test("dotStatus stays connected in setup once capture has been proven", () => {
  const reopened = reduce(reduce(onboarding, { t: "captured" }), { t: "re-invoke", at: 2_000 });
  expect(reopened.phase).toBe("onboarding");
  expect(dotStatus(reopened)).toBe("connected");
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

test("stepStatus derives done/active/pending from step order", () => {
  const s: OnbState = { ...onboarding, step: "set-proxy" };
  expect(stepStatus(s, "trust-ca")).toBe("done");
  expect(stepStatus(s, "set-proxy")).toBe("active");
  expect(stepStatus(s, "verify")).toBe("pending");
});
