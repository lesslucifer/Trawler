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

test("next/back walk the steps and clamp at the ends", () => {
  const s1 = reduce(onboarding, { t: "next" });
  expect(s1.step).toBe("set-proxy");
  const s2 = reduce(s1, { t: "next" });
  expect(s2.step).toBe("verify");
  expect(reduce(s2, { t: "next" }).step).toBe("verify"); // clamped
  expect(reduce(onboarding, { t: "back" }).step).toBe("trust-ca"); // clamped
});

test("goto jumps straight to a step", () => {
  expect(reduce(onboarding, { t: "goto", step: "verify" }).step).toBe("verify");
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
