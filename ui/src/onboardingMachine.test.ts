import { expect, test } from "vitest";
import { dotStatus, initial, reduce, stepStatus, type OnbState } from "./onboardingMachine";

const onboarding: OnbState = { phase: "onboarding", step: "trust-ca", everProven: false };

test("initial() is checking@trust-ca, never proven", () => {
  expect(initial()).toEqual({ phase: "checking", step: "trust-ca", everProven: false });
});

test("not-detected drops checking into onboarding at the first step", () => {
  expect(reduce(initial(), { t: "not-detected" })).toMatchObject({
    phase: "onboarding",
    step: "trust-ca",
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
  expect(reduce(proven, { t: "re-invoke" })).toMatchObject({
    phase: "onboarding",
    step: "trust-ca",
    everProven: true,
  });
});

test("exit returns to proven only once ever proven, else no-op", () => {
  expect(reduce(onboarding, { t: "exit" })).toBe(onboarding); // never proven → no-op
  const reopened = reduce(reduce(onboarding, { t: "captured" }), { t: "re-invoke" });
  expect(reduce(reopened, { t: "exit" }).phase).toBe("proven");
});

test("dotStatus maps each phase", () => {
  expect(dotStatus({ ...onboarding, phase: "checking" })).toBe("checking");
  expect(dotStatus({ ...onboarding, phase: "onboarding" })).toBe("broken");
  expect(dotStatus({ ...onboarding, phase: "proven" })).toBe("connected");
});

test("stepStatus derives done/active/pending from step order", () => {
  const s: OnbState = { ...onboarding, step: "set-proxy" };
  expect(stepStatus(s, "trust-ca")).toBe("done");
  expect(stepStatus(s, "set-proxy")).toBe("active");
  expect(stepStatus(s, "verify")).toBe("pending");
});
