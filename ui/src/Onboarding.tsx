import { useState } from "react";
import { COPY, STEPS, stepStatus, type OnbEvent, type OnbState, type Step } from "./onboardingMachine";
import { Button, CopyText, Disclosure, Notice, StatusLine, type NoticeTone } from "./components/index";
import { ManualFallback, ManualSteps, TargetPicker } from "./manualSteps";
import {
  canAutoSetup,
  defaultProxyTarget,
  defaultTarget,
  guidedSteps,
  proveItSnippet,
  proxyOffSnippet,
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

// The onboarding surface: a checklist of preconditions, each carrying a probed
// verdict. Not a wizard — the steps are re-entered out of order, so there is no
// Back/Next pair, and completion is unobservable: `stepStatus` decides which
// step is OPEN, never whether one is DONE.

// The local action states are proven-by-running only: done/failed report just
// that the elevated command ran (or didn't). Capture is proven solely by a live
// flow, so neither action ever transitions the machine.
type ActionState = "idle" | "running" | "done" | "failed";

interface ActionNote {
  tone: NoticeTone;
  text: string;
}

const INSTALL_NOTE: Partial<Record<ActionState, ActionNote>> = {
  running: { tone: "mute", text: "installing — approve the system prompt…" },
  done: { tone: "ok", text: "installed. we'll confirm it on your first flow." },
  failed: { tone: "warn", text: "automatic install failed — use the manual steps below." },
};

const SETPROXY_NOTE: Partial<Record<ActionState, ActionNote>> = {
  running: { tone: "mute", text: "setting the proxy — approve the prompt if asked…" },
  done: { tone: "ok", text: "proxy set. we'll confirm it on your first flow." },
  failed: { tone: "warn", text: "automatic setup failed — use the manual steps below." },
};

const UNSETPROXY_NOTE: Partial<Record<ActionState, ActionNote>> = {
  running: { tone: "mute", text: "turning the proxy off — approve the prompt if asked…" },
  done: { tone: "ok", text: "proxy off. set it again to resume routing through Trawler." },
  failed: { tone: "warn", text: "couldn't turn the proxy off — follow the steps below instead." },
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

// Fork D4: disabled in flight AND a progress line beside it — both halves or
// neither, so a dead control is never the only signal.
function Action({
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
  note: Partial<Record<ActionState, ActionNote>>;
  onClick: () => void;
}) {
  const shown = note[state];
  return (
    <div>
      <Button onClick={onClick} disabled={state === "running"}>
        {label}
      </Button>
      {shown && (
        <div data-testid={`${testId}-status`} data-state={state} className="mt-chrome-y">
          <Notice tone={shown.tone} detail={state === "failed" ? error : undefined}>
            {shown.text}
          </Notice>
        </div>
      )}
    </div>
  );
}

function TrustCa({
  os,
  state,
  error,
  remedy,
  onInstall,
}: {
  os: HostOs;
  state: ActionState;
  error?: string;
  remedy?: string;
  onInstall?: () => void;
}) {
  const [target, setTarget] = useState<TrustTarget>(defaultTarget(os));
  const auto = onInstall && canAutoSetup(os) && target === defaultTarget(os);
  // The action's own Notice already carries the error, but only while it is
  // mounted — switching target unmounts it, so the remedy takes over rather than
  // leaving the failure stated nowhere.
  const statedByAction = Boolean(auto) && state === "failed";

  return (
    <div className="mt-2">
      {auto && (
        <Action
          testId="install-ca"
          label={COPY.steps["trust-ca"].action}
          state={state}
          error={error}
          note={INSTALL_NOTE}
          onClick={onInstall}
        />
      )}
      {remedy && !statedByAction && (
        <div data-testid="trust-ca-remedy" className="mb-2">
          <Notice>{remedy}</Notice>
        </div>
      )}
      <ManualFallback
        summary="set it up manually"
        collapsed={Boolean(auto)}
        defaultOpen={state === "failed"}
        testId="manual"
        className="mt-2"
      >
        <TargetPicker
          options={TARGETS}
          value={target}
          onChange={(id) => setTarget(id as TrustTarget)}
          label="trust target"
        />
        <ManualSteps steps={guidedSteps(target)} testId="guided-steps" />
        <div className="mt-2">
          <Disclosure summary="remove the certificate later">
            <ManualSteps steps={removalSteps(target)} testId="removal-steps" />
          </Disclosure>
        </div>
      </ManualFallback>
    </div>
  );
}

function SetProxy({
  os,
  addr,
  state,
  error,
  remedy,
  onSetProxy,
  unset,
}: {
  os: HostOs;
  addr: string;
  state: ActionState;
  error?: string;
  remedy?: string;
  onSetProxy?: () => void;
  unset: ReturnType<typeof useAction>;
}) {
  const [target, setTarget] = useState<ProxyTarget>(defaultProxyTarget(os));
  const auto = onSetProxy && canAutoSetup(os) && target === defaultProxyTarget(os);
  const snippet = proxySnippet(target, addr);
  const statedByAction = Boolean(auto) && state === "failed";
  // Offered the moment a native set succeeds; the stop action turns the system
  // proxy back off (idempotent — no-op if it isn't set).
  const showStop = auto && state === "done";

  return (
    <div className="mt-2">
      {auto && (
        <div className="mb-2 flex flex-wrap gap-chrome-gap">
          <Action
            testId="set-proxy"
            label={COPY.steps["set-proxy"].action}
            state={state}
            error={error}
            note={SETPROXY_NOTE}
            onClick={onSetProxy}
          />
          {showStop && unset.trigger && (
            <Action
              testId="unset-proxy"
              label="stop routing through Trawler"
              state={unset.state}
              error={unset.error}
              note={UNSETPROXY_NOTE}
              onClick={unset.trigger}
            />
          )}
        </div>
      )}
      {remedy && !statedByAction && (
        <div data-testid="set-proxy-remedy" className="mb-2">
          <Notice>{remedy}</Notice>
        </div>
      )}
      <ManualFallback
        summary="set it up manually"
        collapsed={Boolean(auto)}
        defaultOpen={state === "failed"}
        testId="manual"
        className="mt-2"
      >
        <TargetPicker
          options={PROXY_TARGETS}
          value={target}
          onChange={(id) => setTarget(id as ProxyTarget)}
          label="proxy target"
        />
        <ManualSteps
          steps={proxySteps(target, addr)}
          command={snippet}
          commandLabel="proxy command"
          testId="proxy-steps"
        />
        <div className="mt-2">
          <Disclosure summary="turn the proxy off later">
            <ManualSteps
              steps={proxyOffSteps(target)}
              command={proxyOffSnippet(target)}
              commandLabel="proxy-off command"
              testId="proxy-off-steps"
            />
          </Disclosure>
        </div>
      </ManualFallback>
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
  const install = useAction(onInstallCa);
  const setProxyAction = useAction(onSetProxy);
  const unsetProxyAction = useAction(onUnsetProxy);

  // The honest per-step verdict: a local probe or observed traffic, not the
  // stepper's cursor.
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

  return (
    <div className="flex h-full flex-col overflow-auto">
      <div className="flex justify-end px-4 py-2">
        {state.everProven && (
          <Button onClick={() => dispatch({ t: "exit" })} frame="bare" tone="mute">
            {COPY.back}
          </Button>
        )}
      </div>

      <div className="flex flex-1 items-center justify-center p-6">
        <div className="w-full max-w-dialog-max">
          <h2 className="text-center font-serif text-title italic text-ink">{COPY.title}</h2>
          <p className="mb-4 text-center font-ui text-note text-ink-mute">{COPY.subtitle}</p>

          {STEPS.map((step, i) => {
            const status = stepStatus(state, step);
            const open = status === "active";
            const copy = COPY.steps[step];
            const view = statusOf(step);
            return (
              <div
                key={step}
                data-testid={`step-${step}`}
                data-status={status}
                data-tone={view.tone}
                className={`my-2 rounded-lg border px-3 py-2 ${
                  open ? "border-ink-mute" : "border-hairline"
                }`}
              >
                <button
                  type="button"
                  onClick={() => dispatch({ t: "goto", step })}
                  className="flex w-full items-baseline justify-between gap-2 text-left font-ui text-body text-ink"
                >
                  <span>
                    {i + 1} · {copy.title}
                  </span>
                  <span data-testid={`step-${step}-status`} className="shrink-0">
                    <StatusLine status={view} />
                  </span>
                </button>

                {open && (
                  <div data-testid="step-body" className="mt-2">
                    <p className="font-ui text-body text-ink-mute">{copy.body}</p>
                    {step === "trust-ca" ? (
                      <TrustCa
                        os={os}
                        state={install.state}
                        error={install.error}
                        remedy={view.remedy}
                        onInstall={install.trigger}
                      />
                    ) : step === "set-proxy" ? (
                      <SetProxy
                        os={os}
                        addr={proxyAddr}
                        state={setProxyAction.state}
                        error={setProxyAction.error}
                        remedy={view.remedy}
                        onSetProxy={setProxyAction.trigger}
                        unset={unsetProxyAction}
                      />
                    ) : (
                      <div className="mt-2">
                        <CopyText value={proveItSnippet(proxyAddr)} label="prove-it command" />
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
