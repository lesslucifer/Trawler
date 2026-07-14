import { useState, type ReactNode } from "react";
import { COPY, STEPS, stepStatus, type OnbEvent, type OnbState, type Step } from "./onboardingMachine";
import {
  canAutoSetup,
  defaultProxyTarget,
  defaultTarget,
  guidedSteps,
  proveItSnippet,
  proxyOffSteps,
  proxySnippet,
  proxyStatus,
  proxySteps,
  PROXY_TARGETS,
  removalSteps,
  TARGETS,
  trustStatus,
  verifyStatus,
  type HostOs,
  type Observed,
  type ProxyProbe,
  type ProxyTarget,
  type Running,
  type StepStatusView,
  type TrustProbe,
  type TrustTarget,
} from "./setup";

// The onboarding surface: a centered stepper rendered from the state machine.
// Trust-ca (W2) and set-proxy (W3) are live; live verify (W4) lands later.
// Back to flows shows only once a list exists.

// The local action states are proven-by-running only: done/failed report just
// that the elevated command ran (or didn't). Capture is proven solely by W4's
// live flow, so neither action ever transitions the machine.
type ActionState = "idle" | "running" | "done" | "failed";

const INSTALL_NOTE: Record<ActionState, string | null> = {
  idle: null,
  running: "Installing — approve the system prompt…",
  done: "Installed. We'll confirm it's working at the Verify step.",
  failed: "Automatic install failed — use the manual steps below.",
};

const SETPROXY_NOTE: Record<ActionState, string | null> = {
  idle: null,
  running: "Setting the proxy — approve the prompt if asked…",
  done: "Proxy set. We'll confirm it's working at the Verify step.",
  failed: "Automatic setup failed — use the manual steps below.",
};

const UNSETPROXY_NOTE: Record<ActionState, string | null> = {
  idle: null,
  running: "Turning the proxy off — approve the prompt if asked…",
  done: "Proxy off. Set it again to resume routing through Trawler.",
  failed: "Couldn't turn the proxy off — follow the steps below instead.",
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

// P3 — the manual steps. When a one-click action exists (`collapsed`), the whole
// per-OS block hides behind a "Set it up manually" disclosure so the happy path
// isn't buried; otherwise it renders inline (no auto path, so it's the only way).
// A failed auto action force-opens it — the failure note says "use the manual
// steps below", so they must be visible when it does.
function ManualPanel({
  collapsed,
  open,
  children,
}: {
  collapsed: boolean;
  open?: boolean;
  children: ReactNode;
}) {
  if (!collapsed) return <div className="mt-2">{children}</div>;
  return (
    <details data-testid="manual" className="mt-2" open={open}>
      <summary className="cursor-pointer text-ink-mute">Set it up manually</summary>
      <div className="mt-1">{children}</div>
    </details>
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
      <ManualPanel collapsed={Boolean(auto)} open={state === "failed"}>
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
      </ManualPanel>
    </div>
  );
}

function SetProxy({
  os,
  addr,
  state,
  error,
  onSetProxy,
  unset,
}: {
  os: HostOs;
  addr: string;
  state: ActionState;
  error?: string;
  onSetProxy?: () => void;
  unset: ReturnType<typeof useAction>;
}) {
  const [target, setTarget] = useState<ProxyTarget>(defaultProxyTarget(os));
  const auto = onSetProxy && canAutoSetup(os) && target === defaultProxyTarget(os);
  const snippet = proxySnippet(target, addr);
  // Reverting is marker-gated in Rust, so this only ever turns off a proxy Trawler
  // set — safe to offer the moment a native set succeeds.
  const showStop = auto && state === "done";

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
      {showStop && unset.trigger && (
        <ActionButton
          testId="unset-proxy"
          label="Stop routing through Trawler"
          state={unset.state}
          error={unset.error}
          note={UNSETPROXY_NOTE}
          onClick={unset.trigger}
        />
      )}
      <ManualPanel collapsed={Boolean(auto)} open={state === "failed"}>
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
      </ManualPanel>
    </div>
  );
}

// P1 — the Verify step's prove-it: a concrete, copyable command that generates
// the first request. Text only; the app never fires it (epic non-goal). Success
// still flips green from a real captured flow (App watches the stream).
function ProveIt({ addr }: { addr: string }) {
  const cmd = proveItSnippet(addr);
  return (
    <div className="mt-2">
      <pre data-testid="prove-it" className="overflow-x-auto rounded-button border border-hairline px-2 py-1 text-ink">
        {cmd}
      </pre>
      <button
        type="button"
        data-testid="copy-prove-it"
        onClick={() => navigator.clipboard.writeText(cmd)}
        className="mt-1 rounded-button border border-hairline px-2 py-1 text-ink"
      >
        Copy command
      </button>
    </div>
  );
}

export function Onboarding({
  state,
  dispatch,
  os = "other",
  proxyAddr = "127.0.0.1:7420",
  trustProbe = "unknown",
  proxyProbe = "unknown",
  observed = { decrypted: false, trafficSeen: false },
  onInstallCa,
  onSetProxy,
  onUnsetProxy,
}: {
  state: OnbState;
  dispatch: (e: OnbEvent) => void;
  os?: HostOs;
  proxyAddr?: string;
  trustProbe?: TrustProbe;
  proxyProbe?: ProxyProbe;
  observed?: Observed;
  onInstallCa?: () => Promise<void>;
  onSetProxy?: () => Promise<void>;
  onUnsetProxy?: () => Promise<void>;
}) {
  const atFirst = state.step === STEPS[0];
  const atLast = state.step === STEPS[STEPS.length - 1];
  const install = useAction(onInstallCa);
  const setProxyAction = useAction(onSetProxy);
  const unsetProxyAction = useAction(onUnsetProxy);

  // The honest per-step status (W6): a local probe or observed traffic, not the
  // stepper's cursor. `stepStatus` still decides which step's body is expanded.
  const running = (r: Running, active: boolean) => (active ? r : null);
  const statusOf = (step: Step): StepStatusView => {
    if (step === "trust-ca")
      return trustStatus(
        trustProbe,
        observed,
        running("installing", install.state === "running"),
        install.state === "failed" ? install.error : undefined,
      );
    if (step === "set-proxy")
      return proxyStatus(
        proxyProbe,
        observed,
        running("setting", setProxyAction.state === "running"),
        setProxyAction.state === "failed" ? setProxyAction.error : undefined,
      );
    return verifyStatus(observed);
  };
  const toneClass = { ok: "text-ok", warn: "text-warn", muted: "text-ink-mute" } as const;

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
              const active = stepStatus(state, step) === "active";
              const copy = COPY.steps[step];
              const view = statusOf(step);
              return (
                <li
                  key={step}
                  data-testid={`step-${step}`}
                  data-status={stepStatus(state, step)}
                  data-tone={view.tone}
                  className={`my-2 rounded-lg border px-3 py-2 ${
                    active ? "border-ink-mute" : "border-hairline"
                  }`}
                >
                  <button
                    type="button"
                    onClick={() => dispatch({ t: "goto", step })}
                    className="flex w-full items-center justify-between gap-2 text-left text-ink"
                  >
                    <span>
                      {i + 1} · {copy.title}
                    </span>
                    <small data-testid={`step-${step}-status`} className={`text-right ${toneClass[view.tone]}`}>
                      {view.glyph} {view.message}
                    </small>
                  </button>

                  {active && (
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
                          unset={unsetProxyAction}
                        />
                      ) : (
                        <ProveIt addr={proxyAddr} />
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
