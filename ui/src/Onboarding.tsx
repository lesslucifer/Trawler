import { useState } from "react";
import { COPY, STEPS, stepStatus, type OnbEvent, type OnbState } from "./onboardingMachine";
import {
  canAutoInstall,
  defaultTarget,
  guidedSteps,
  TARGETS,
  type HostOs,
  type TrustTarget,
} from "./setup";

// The onboarding surface: a centered stepper rendered from the state machine.
// Trust-ca is live (W2); proxy set (W3) and live verify (W4) land later.
// Back to flows shows only once a list exists.

const GLYPH = { done: "✓ done", active: "▸ active", pending: "○ waiting" } as const;

// done/failed report only that the elevated command ran (or didn't) — trust is
// proven solely by W4's live capture, so the machine never transitions here.
const INSTALL_NOTE = {
  idle: null,
  installing: "Installing — approve the system prompt…",
  done: "Command ran. The Verify step will prove capture works.",
  failed: "Install didn't finish — follow the steps below instead.",
} as const;

type InstallState = keyof typeof INSTALL_NOTE;

function TrustCa({
  os,
  install,
  error,
  onInstall,
}: {
  os: HostOs;
  install: InstallState;
  error?: string;
  onInstall?: () => void;
}) {
  const [target, setTarget] = useState<TrustTarget>(defaultTarget(os));
  const auto = onInstall && canAutoInstall(os) && target === defaultTarget(os);

  return (
    <div className="mt-2">
      {auto && (
        <div className="mb-2">
          <button
            type="button"
            data-testid="install-ca"
            disabled={install === "installing"}
            onClick={onInstall}
            className="rounded-button border border-hairline px-2 py-1 text-ink disabled:opacity-60"
          >
            {COPY.steps["trust-ca"].action}
          </button>
          {INSTALL_NOTE[install] && (
            <div data-testid="install-status" data-install={install} className="mt-1">
              <p>{INSTALL_NOTE[install]}</p>
              {install === "failed" && error && (
                <p data-testid="install-error" className="text-warn">
                  {error}
                </p>
              )}
            </div>
          )}
        </div>
      )}
      <label className="flex items-center gap-2">
        Steps for
        <select
          data-testid="trust-target"
          value={target}
          onChange={(e) => setTarget(e.target.value as TrustTarget)}
          className="rounded-button border border-hairline bg-transparent px-1 py-0.5 text-ink"
        >
          {TARGETS.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
            </option>
          ))}
        </select>
      </label>
      <ol data-testid="guided-steps" className="mt-1 list-decimal pl-5">
        {guidedSteps(target).map((step) => (
          <li key={step}>{step}</li>
        ))}
      </ol>
    </div>
  );
}

export function Onboarding({
  state,
  dispatch,
  os = "other",
  onInstallCa,
}: {
  state: OnbState;
  dispatch: (e: OnbEvent) => void;
  os?: HostOs;
  onInstallCa?: () => Promise<void>;
}) {
  const atFirst = state.step === STEPS[0];
  const atLast = state.step === STEPS[STEPS.length - 1];
  const [install, setInstall] = useState<InstallState>("idle");
  const [installError, setInstallError] = useState<string>();
  const runInstall =
    onInstallCa &&
    (() => {
      setInstall("installing");
      setInstallError(undefined);
      onInstallCa().then(
        () => setInstall("done"),
        (err: unknown) => {
          console.error("CA install failed:", err);
          setInstall("failed");
          setInstallError(err instanceof Error ? err.message : String(err));
        },
      );
    });

  return (
    <div className="flex h-full flex-col overflow-auto">
      <div className="flex justify-end px-4 py-2">
        {state.everProven && (
          <button
            type="button"
            data-testid="onb-exit"
            onClick={() => dispatch({ t: "exit" })}
            className="rounded-button px-2 py-1 text-sm text-ink-mute hover:text-ink"
          >
            {COPY.back}
          </button>
        )}
      </div>

      <div className="flex flex-1 items-center justify-center p-6">
        <div className="w-full max-w-sm">
          <h2 className="text-center font-serif text-lg italic text-ink">{COPY.title}</h2>
          <p className="mb-4 text-center text-sm text-ink-mute">{COPY.subtitle}</p>

          <ol>
            {STEPS.map((step, i) => {
              const status = stepStatus(state, step);
              const copy = COPY.steps[step];
              const installFailed = step === "trust-ca" && install === "failed";
              const glyph =
                installFailed && status === "done" ? "✗ failed" : GLYPH[status];
              return (
                <li
                  key={step}
                  data-testid={`step-${step}`}
                  data-status={status}
                  className={`my-2 rounded-lg border px-3 py-2 ${
                    status === "active" ? "border-ink-mute" : "border-hairline"
                  }`}
                >
                  <button
                    type="button"
                    onClick={() => dispatch({ t: "goto", step })}
                    className="flex w-full items-center justify-between text-left text-ink"
                  >
                    <span>
                      {i + 1} · {copy.title}
                    </span>
                    <small className={glyph === "✗ failed" ? "text-warn" : "text-ink-mute"}>
                      {glyph}
                    </small>
                  </button>

                  {status === "active" && (
                    <div data-testid="step-body" className="mt-2 text-sm text-ink-mute">
                      <p>{copy.body}</p>
                      {step === "trust-ca" ? (
                        <TrustCa
                          os={os}
                          install={install}
                          error={installError}
                          onInstall={runInstall}
                        />
                      ) : (
                        copy.action && (
                          <button
                            type="button"
                            disabled
                            title="Available in a later step of setup"
                            className="mt-2 rounded-button border border-hairline px-2 py-1 text-ink opacity-60"
                          >
                            {copy.action}
                          </button>
                        )
                      )}
                    </div>
                  )}
                </li>
              );
            })}
          </ol>

          <div className="mt-3 flex justify-between">
            <button
              type="button"
              data-testid="onb-back"
              disabled={atFirst}
              onClick={() => dispatch({ t: "back" })}
              className="rounded-button border border-hairline px-3 py-1 text-ink disabled:opacity-40"
            >
              Back
            </button>
            <button
              type="button"
              data-testid="onb-next"
              disabled={atLast}
              onClick={() => dispatch({ t: "next" })}
              className="rounded-button border border-hairline px-3 py-1 text-ink disabled:opacity-40"
            >
              Next
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
