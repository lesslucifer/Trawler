import { useState } from "react";
import { COPY, STEPS, stepStatus, type OnbEvent, type OnbState } from "./onboardingMachine";
import {
  canAutoSetup,
  defaultProxyTarget,
  defaultTarget,
  guidedSteps,
  proxyOffSteps,
  proxySnippet,
  proxySteps,
  PROXY_TARGETS,
  removalSteps,
  TARGETS,
  type HostOs,
  type ProxyTarget,
  type TrustTarget,
} from "./setup";

// The onboarding surface: a centered stepper rendered from the state machine.
// Trust-ca (W2) and set-proxy (W3) are live; live verify (W4) lands later.
// Back to flows shows only once a list exists.

const GLYPH = { done: "✓ done", active: "▸ active", pending: "○ waiting" } as const;

// The local action states are proven-by-running only: done/failed report just
// that the elevated command ran (or didn't). Capture is proven solely by W4's
// live flow, so neither action ever transitions the machine.
type ActionState = "idle" | "running" | "done" | "failed";

const INSTALL_NOTE: Record<ActionState, string | null> = {
  idle: null,
  running: "Installing — approve the system prompt…",
  done: "Command ran. The Verify step will prove capture works.",
  failed: "Install didn't finish — follow the steps below instead.",
};

const SETPROXY_NOTE: Record<ActionState, string | null> = {
  idle: null,
  running: "Setting the proxy — approve the prompt if asked…",
  done: "Proxy set. The Verify step will prove capture works.",
  failed: "Couldn't set the proxy — follow the steps below instead.",
};

function useAction(run?: () => Promise<void>) {
  const [state, setState] = useState<ActionState>("idle");
  const [error, setError] = useState<string>();
  const trigger =
    run &&
    (() => {
      setState("running");
      setError(undefined);
      run().then(
        () => setState("done"),
        (err: unknown) => {
          console.error("setup action failed:", err);
          setState("failed");
          setError(err instanceof Error ? err.message : String(err));
        },
      );
    });
  return { state, error, trigger };
}

function ActionButton({
  testId,
  label,
  state,
  error,
  note,
  onClick,
}: {
  testId: string;
  label: string;
  state: ActionState;
  error?: string;
  note: Record<ActionState, string | null>;
  onClick: () => void;
}) {
  return (
    <div className="mb-2">
      <button
        type="button"
        data-testid={testId}
        disabled={state === "running"}
        onClick={onClick}
        className="rounded-button border border-hairline px-2 py-1 text-ink disabled:opacity-60"
      >
        {label}
      </button>
      {note[state] && (
        <div data-testid={`${testId}-status`} data-state={state} className="mt-1">
          <p>{note[state]}</p>
          {state === "failed" && error && (
            <p data-testid={`${testId}-error`} className="text-warn">
              {error}
            </p>
          )}
        </div>
      )}
    </div>
  );
}

function TrustCa({
  os,
  state,
  error,
  onInstall,
}: {
  os: HostOs;
  state: ActionState;
  error?: string;
  onInstall?: () => void;
}) {
  const [target, setTarget] = useState<TrustTarget>(defaultTarget(os));
  const auto = onInstall && canAutoSetup(os) && target === defaultTarget(os);

  return (
    <div className="mt-2">
      {auto && (
        <ActionButton
          testId="install-ca"
          label={COPY.steps["trust-ca"].action}
          state={state}
          error={error}
          note={INSTALL_NOTE}
          onClick={onInstall}
        />
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
      <details className="mt-2">
        <summary className="cursor-pointer text-ink-mute">Remove the certificate later</summary>
        <ol data-testid="removal-steps" className="mt-1 list-decimal pl-5">
          {removalSteps(target).map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
      </details>
    </div>
  );
}

function SetProxy({
  os,
  addr,
  state,
  error,
  onSetProxy,
}: {
  os: HostOs;
  addr: string;
  state: ActionState;
  error?: string;
  onSetProxy?: () => void;
}) {
  const [target, setTarget] = useState<ProxyTarget>(defaultProxyTarget(os));
  const auto = onSetProxy && canAutoSetup(os) && target === defaultProxyTarget(os);
  const snippet = proxySnippet(target, addr);

  return (
    <div className="mt-2">
      {auto && (
        <ActionButton
          testId="set-proxy"
          label={COPY.steps["set-proxy"].action}
          state={state}
          error={error}
          note={SETPROXY_NOTE}
          onClick={onSetProxy}
        />
      )}
      <label className="flex items-center gap-2">
        Steps for
        <select
          data-testid="proxy-target"
          value={target}
          onChange={(e) => setTarget(e.target.value as ProxyTarget)}
          className="rounded-button border border-hairline bg-transparent px-1 py-0.5 text-ink"
        >
          {PROXY_TARGETS.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
            </option>
          ))}
        </select>
      </label>
      <ol data-testid="proxy-steps" className="mt-1 list-decimal pl-5">
        {proxySteps(target, addr).map((step) => (
          <li key={step}>{step}</li>
        ))}
      </ol>
      {snippet && (
        <button
          type="button"
          data-testid="copy-snippet"
          onClick={() => navigator.clipboard.writeText(snippet)}
          className="mt-1 rounded-button border border-hairline px-2 py-1 text-ink"
        >
          Copy command
        </button>
      )}
      <details className="mt-2">
        <summary className="cursor-pointer text-ink-mute">Turn the proxy off later</summary>
        <ol data-testid="proxy-off-steps" className="mt-1 list-decimal pl-5">
          {proxyOffSteps(target).map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
      </details>
    </div>
  );
}

export function Onboarding({
  state,
  dispatch,
  os = "other",
  proxyAddr = "127.0.0.1:7420",
  onInstallCa,
  onSetProxy,
}: {
  state: OnbState;
  dispatch: (e: OnbEvent) => void;
  os?: HostOs;
  proxyAddr?: string;
  onInstallCa?: () => Promise<void>;
  onSetProxy?: () => Promise<void>;
}) {
  const atFirst = state.step === STEPS[0];
  const atLast = state.step === STEPS[STEPS.length - 1];
  const install = useAction(onInstallCa);
  const setProxyAction = useAction(onSetProxy);

  const failedState = (step: string) =>
    (step === "trust-ca" && install.state === "failed") ||
    (step === "set-proxy" && setProxyAction.state === "failed");

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
              const showFailed = failedState(step) && status === "done";
              const glyph = showFailed ? "✗ failed" : GLYPH[status];
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
                    <small className={showFailed ? "text-warn" : "text-ink-mute"}>{glyph}</small>
                  </button>

                  {status === "active" && (
                    <div data-testid="step-body" className="mt-2 text-sm text-ink-mute">
                      <p>{copy.body}</p>
                      {step === "trust-ca" ? (
                        <TrustCa
                          os={os}
                          state={install.state}
                          error={install.error}
                          onInstall={install.trigger}
                        />
                      ) : step === "set-proxy" ? (
                        <SetProxy
                          os={os}
                          addr={proxyAddr}
                          state={setProxyAction.state}
                          error={setProxyAction.error}
                          onSetProxy={setProxyAction.trigger}
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
