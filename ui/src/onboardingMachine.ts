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
  | { t: "next" }
  | { t: "back" }
  | { t: "goto"; step: Step }
  | { t: "captured" } // fresh decrypted HTTPS flow — W4 detector dispatches
  | { t: "not-detected" } // launch self-test found nothing — W4 dispatches (W1: on mount)
  | { t: "re-invoke" } // tap status dot / W4 health-check fails → re-setup
  | { t: "exit" }; // Back to flows — returns to proven iff everProven, else no-op

export const STEPS: readonly Step[] = ["trust-ca", "set-proxy", "verify"];

export function initial(): OnbState {
  return { phase: "checking", step: "trust-ca", everProven: false };
}

function shift(step: Step, delta: number): Step {
  const i = STEPS.indexOf(step);
  return STEPS[Math.min(STEPS.length - 1, Math.max(0, i + delta))];
}

export function reduce(s: OnbState, e: OnbEvent): OnbState {
  switch (e.t) {
    case "next":
      return { ...s, step: shift(s.step, 1) };
    case "back":
      return { ...s, step: shift(s.step, -1) };
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
  title: "Set up Trawler",
  subtitle: "Capture your first HTTPS request",
  steps: {
    "trust-ca": {
      title: "Trust the CA",
      body: "To read HTTPS traffic, Trawler signs it with its own certificate. Your machine needs to trust that certificate first.",
      action: "Install certificate",
    },
    "set-proxy": {
      title: "Set the proxy",
      body: "Send your traffic through Trawler. One click on macOS and Windows; a copy-and-paste command elsewhere.",
      action: "Set system proxy",
    },
    verify: {
      title: "Verify",
      body: "Almost there — now generate some traffic. Open any website, or run the command below. Your first request will show up here.",
      action: null,
    },
  },
  back: "‹ Back to flows",
  dotTitle: {
    broken: "Not capturing yet — click to set up",
    checking: "Checking…",
    connected: "Capturing — click to run setup again",
  },
} as const;
