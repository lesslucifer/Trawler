// Onboarding state machine — pure, no React, no I/O, so its transitions are
// unit-testable with no network (E-UISETUP.W1). The surface renders from it;
// W4 swaps the mount placeholder for a real scheduled capture self-test.

export type Step = "trust-ca" | "set-proxy" | "verify";
export type Phase = "checking" | "onboarding" | "proven";
export type DotStatus = "broken" | "checking" | "connected";

export interface OnbState {
  phase: Phase;
  step: Step;
  everProven: boolean;
}

export type OnbEvent =
  | { t: "goto"; step: Step }
  | { t: "captured" } // fresh decrypted HTTPS flow — W4 detector dispatches
  | { t: "not-detected" } // launch self-test found nothing — W4 dispatches (W1: on mount)
  | { t: "re-invoke" } // tap status dot / W4 health-check fails → re-setup
  | { t: "exit" }; // Back to flows — returns to proven iff everProven, else no-op

export const STEPS: readonly Step[] = ["trust-ca", "set-proxy", "verify"];

export function initial(): OnbState {
  return { phase: "checking", step: "trust-ca", everProven: false };
}

export function reduce(s: OnbState, e: OnbEvent): OnbState {
  switch (e.t) {
    case "goto":
      return { ...s, step: e.step };
    case "captured":
      return { ...s, phase: "proven", everProven: true };
    case "not-detected":
    case "re-invoke":
      return { ...s, phase: "onboarding", step: "trust-ca" };
    case "exit":
      return s.everProven ? { ...s, phase: "proven" } : s;
  }
}

// "done" is proven by capture order, never self-reported: steps before the
// current one read done, the current one active, the rest pending.
export type StepStatus = "done" | "active" | "pending";
export function stepStatus(s: OnbState, step: Step): StepStatus {
  const cur = STEPS.indexOf(s.step);
  const idx = STEPS.indexOf(step);
  return idx < cur ? "done" : idx === cur ? "active" : "pending";
}

export function dotStatus(s: OnbState): DotStatus {
  return s.phase === "proven" ? "connected" : s.phase === "checking" ? "checking" : "broken";
}

export const COPY = {
  back: "back to flows",
  dotTitle: {
    broken: "Not capturing yet — click to set up",
    checking: "Checking…",
    connected: "Capturing — click to run setup again",
  },
} as const;
