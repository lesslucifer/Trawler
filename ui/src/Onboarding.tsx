import { useEffect, useState } from "react";
import { COPY, STEPS, stepStatus, type OnbEvent, type OnbState, type Step } from "./onboardingMachine";
import {
  Button,
  CopyText,
  Disclosure,
  IconButton,
  Notice,
  StatusLine,
  TimestampText,
  type NoticeTone,
} from "./components/index";
import { ManualFallback, ManualSteps, TargetPicker } from "./manualSteps";
import {
  defaultProxyTarget,
  proxyOffSnippet,
  proxyOffSteps,
  PROXY_TARGETS,
  type ProxyTarget,
} from "./proxyTargets";
import {
  canAutoSetup,
  CA_FILENAME,
  defaultTarget,
  guidedSteps,
  proveItSnippet,
  proxySnippet,
  proxySteps,
  removalSnippet,
  removalSteps,
  SAVE_CA_STEP,
  TARGETS,
  trustSnippet,
  type HostOs,
  type TrustTarget,
} from "./setup";
import {
  NO_TRAFFIC,
  proxyStatus,
  trafficStatus,
  trustStatus,
  type ProxyProbe,
  type Running,
  type StepStatusView,
  type Traffic,
  type TrustProbe,
} from "./verdicts";

// The onboarding surface: a checklist of preconditions, each carrying a verdict.
// Not a wizard — the steps are re-entered out of order, so there is no Back/Next
// pair, and completion is unobservable: `stepStatus` decides which step is OPEN,
// never whether one is DONE.
//
// Two layers of truth, never mixed. Steps 1 and 2 report THIS MACHINE from the
// two probes, re-read by one global "check again" and whenever the window
// regains focus. Step 3 reports what is reaching Trawler NOW, from timestamps, so
// a flow captured an hour ago can only ever read as idle.

// The local action states are proven-by-running only: done/failed report just
// that the elevated command ran (or didn't). Capture is proven solely by a live
// flow, so neither action ever transitions the machine.
// `nothing` is a run that resolved without doing anything — a cancelled save
// dialog. Reporting it as done would state an outcome the system never produced.
type ActionState = "idle" | "running" | "done" | "failed" | "nothing";

// An action that resolves a string names what it produced; the note quotes that
// name rather than the one we asked for, since the two differ whenever the user
// renames the file in the save dialog.
type ActionNotes = Partial<Record<ActionState, ActionNote | ((named: string) => ActionNote)>>;

interface ActionNote {
  tone: NoticeTone;
  text: string;
}

const INSTALL_NOTE: ActionNotes = {
  running: { tone: "mute", text: "installing — approve the system prompt…" },
  done: { tone: "ok", text: "installed. we'll confirm it on your first flow." },
  failed: { tone: "warn", text: "automatic install failed — use the manual steps below." },
};

const SETPROXY_NOTE: ActionNotes = {
  running: { tone: "mute", text: "setting the proxy — approve the prompt if asked…" },
  done: { tone: "ok", text: "proxy set. we'll confirm it on your first flow." },
  failed: { tone: "warn", text: "automatic setup failed — use the manual steps below." },
};

// The save resolves a full path natively and a bare name in the browser; the
// steps below name a file, so the note does too.
const fileName = (path: string) => path.split(/[/\\]/).pop() ?? path;

const SAVECA_NOTE: ActionNotes = {
  running: { tone: "mute", text: "saving…" },
  done: (path: string) => ({
    tone: "ok",
    text:
      fileName(path) === CA_FILENAME
        ? `saved as ${CA_FILENAME}. now trust it with the steps below.`
        : `saved as ${fileName(path)} — the steps below name ${CA_FILENAME}, so use ${fileName(path)} instead.`,
  }),
  nothing: { tone: "mute", text: "nothing was saved — the save was cancelled." },
  failed: { tone: "warn", text: "couldn't save the certificate — check that Trawler is running." },
};

const UNSETPROXY_NOTE: ActionNotes = {
  running: { tone: "mute", text: "turning the proxy off — approve the prompt if asked…" },
  done: { tone: "ok", text: "proxy off. set it again to resume routing through Trawler." },
  failed: { tone: "warn", text: "couldn't turn the proxy off — follow the steps below instead." },
};

function useAction(run?: () => Promise<string | null | void>) {
  const [state, setState] = useState<ActionState>("idle");
  const [error, setError] = useState<string>();
  const [named, setNamed] = useState("");
  const trigger =
    run &&
    (() => {
      setState("running");
      setError(undefined);
      run().then(
        (outcome) => {
          if (typeof outcome === "string") setNamed(outcome);
          setState(outcome === null ? "nothing" : "done");
        },
        (err: unknown) => {
          console.error("setup action failed:", err);
          setState("failed");
          setError(err instanceof Error ? err.message : String(err));
        },
      );
    });
  return { state, error, named, trigger };
}

type ActionHandle = ReturnType<typeof useAction>;

const TICK_MS = 1000;

// Recency is judged against a clock that keeps moving while the screen is open,
// or a verdict true at mount would stay true forever.
function useNow(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), TICK_MS);
    return () => clearInterval(id);
  }, []);
  return now;
}

function ActionNote({
  testId,
  action,
  note,
}: {
  testId: string;
  action: ActionHandle;
  note: ActionNotes;
}) {
  const entry = note[action.state];
  const shown = typeof entry === "function" ? entry(action.named) : entry;
  if (!shown) return null;
  return (
    <div data-testid={`${testId}-status`} data-state={action.state} className="mt-chrome-y">
      <Notice tone={shown.tone} detail={action.state === "failed" ? action.error : undefined}>
        {shown.text}
      </Notice>
    </div>
  );
}

// Fork D4: disabled in flight AND a progress line beside it — both halves or
// neither, so a dead control is never the only signal.
function Action({
  testId,
  label,
  action,
  note,
  onClick,
}: {
  testId: string;
  label: string;
  action: ActionHandle;
  note: ActionNotes;
  onClick: () => void;
}) {
  return (
    <div>
      <Button onClick={onClick} disabled={action.state === "running"}>
        {label}
      </Button>
      <ActionNote testId={testId} action={action} note={note} />
    </div>
  );
}

// The step that names the file is the step that produces it: the save control
// ends the sentence, and its outcome lands beneath, inside the same step.
function SaveCertificateStep({ save, onSave }: { save: ActionHandle; onSave: () => void }) {
  return (
    <>
      <span className="flex items-center gap-chrome-gap">
        {SAVE_CA_STEP}
        <IconButton
          glyph="save"
          label="save certificate"
          size={24}
          onClick={onSave}
          disabled={save.state === "running"}
        />
      </span>
      <ActionNote testId="save-ca" action={save} note={SAVECA_NOTE} />
    </>
  );
}

function TrustCa({
  os,
  install,
  remedy,
  onInstall,
  save,
}: {
  os: HostOs;
  install: ActionHandle;
  remedy?: string;
  onInstall?: () => void;
  save: ActionHandle;
}) {
  const [target, setTarget] = useState<TrustTarget>(defaultTarget(os));
  const auto = onInstall && canAutoSetup(os) && target === defaultTarget(os);
  // The action's own Notice already carries the error, but only while it is
  // mounted — switching target unmounts it, so the remedy takes over rather than
  // leaving the failure stated nowhere.
  const statedByAction = Boolean(auto) && install.state === "failed";
  const slots = save.trigger
    ? { [SAVE_CA_STEP]: <SaveCertificateStep save={save} onSave={save.trigger} /> }
    : {};

  return (
    <div className="mt-2">
      {auto && (
        <Action
          testId="install-ca"
          label={COPY.steps["trust-ca"].action}
          action={install}
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
        defaultOpen={install.state === "failed"}
        testId="manual"
        className="mt-2"
      >
        <TargetPicker
          options={TARGETS}
          value={target}
          onChange={(id) => setTarget(id as TrustTarget)}
          label="trust target"
        />
        <ManualSteps
          steps={guidedSteps(target)}
          command={trustSnippet(target)}
          commandLabel="certificate command"
          slots={slots}
          testId="guided-steps"
        />
        <div className="mt-2">
          <Disclosure summary="remove the certificate later">
            <ManualSteps
              steps={removalSteps(target)}
              command={removalSnippet(target)}
              commandLabel="removal command"
              testId="removal-steps"
            />
          </Disclosure>
        </div>
      </ManualFallback>
    </div>
  );
}

function SetProxy({
  os,
  addr,
  set,
  remedy,
  onSetProxy,
  unset,
}: {
  os: HostOs;
  addr: string;
  set: ActionHandle;
  remedy?: string;
  onSetProxy?: () => void;
  unset: ActionHandle;
}) {
  const [target, setTarget] = useState<ProxyTarget>(defaultProxyTarget(os));
  const auto = onSetProxy && canAutoSetup(os) && target === defaultProxyTarget(os);
  const snippet = proxySnippet(target, addr);
  const statedByAction = Boolean(auto) && set.state === "failed";
  // Offered the moment a native set succeeds; the stop action turns the system
  // proxy back off (idempotent — no-op if it isn't set).
  const showStop = auto && set.state === "done";

  return (
    <div className="mt-2">
      {auto && (
        <div className="mb-2 flex flex-wrap gap-chrome-gap">
          <Action
            testId="set-proxy"
            label={COPY.steps["set-proxy"].action}
            action={set}
            note={SETPROXY_NOTE}
            onClick={onSetProxy}
          />
          {showStop && unset.trigger && (
            <Action
              testId="unset-proxy"
              label="stop routing through Trawler"
              action={unset}
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
        defaultOpen={set.state === "failed"}
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

function MakeRequest({
  addr,
  remedy,
  lastFlowAt,
}: {
  addr: string;
  remedy?: string;
  lastFlowAt: number | null;
}) {
  return (
    <div className="mt-2">
      <CopyText value={proveItSnippet(addr)} label="prove-it command" />
      {remedy && (
        <div data-testid="verify-remedy" className="mt-2">
          <Notice>{remedy}</Notice>
        </div>
      )}
      {lastFlowAt !== null && (
        <p data-testid="last-flow" className="mt-2 font-ui text-body text-ink-mute">
          last flow at <TimestampText timestamp={lastFlowAt} />
        </p>
      )}
    </div>
  );
}

// One reading of this machine governs both probed steps, so one control re-takes
// it. The time stamps the reading: a probe is a snapshot, and nothing pushes when
// the user changes a setting elsewhere.
function Recheck({
  probing,
  probedAt,
  onRecheck,
}: {
  probing: boolean;
  probedAt: number | null;
  onRecheck: () => void;
}) {
  return (
    <div
      data-testid="recheck"
      className="mb-2 flex items-center justify-end gap-chrome-gap font-ui text-control text-ink-mute"
    >
      {probing ? (
        <span>checking this machine…</span>
      ) : (
        probedAt !== null && (
          <span>
            checked at <TimestampText timestamp={probedAt} />
          </span>
        )
      )}
      <Button onClick={onRecheck} frame="bare" tone="mute" disabled={probing}>
        check again
      </Button>
    </div>
  );
}

const latest = (a: number | null, b: number | null) =>
  a === null ? b : b === null ? a : Math.max(a, b);

export function Onboarding({
  state,
  dispatch,
  os = "other",
  proxyAddr = "127.0.0.1:7420",
  trustProbe = "unknown",
  proxyProbe = "unknown",
  probing = false,
  probedAt = null,
  traffic = NO_TRAFFIC,
  onInstallCa,
  onSaveCa,
  onSetProxy,
  onUnsetProxy,
  onRecheck,
}: {
  state: OnbState;
  dispatch: (e: OnbEvent) => void;
  os?: HostOs;
  proxyAddr?: string;
  trustProbe?: TrustProbe;
  proxyProbe?: ProxyProbe;
  probing?: boolean;
  probedAt?: number | null;
  traffic?: Traffic;
  onInstallCa?: () => Promise<void>;
  onSaveCa?: () => Promise<string | null>;
  onSetProxy?: () => Promise<void>;
  onUnsetProxy?: () => Promise<void>;
  onRecheck?: () => void;
}) {
  const install = useAction(onInstallCa);
  const saveCa = useAction(onSaveCa);
  const setProxyAction = useAction(onSetProxy);
  const unsetProxyAction = useAction(onUnsetProxy);
  const now = useNow();

  // The action the user just took outranks the re-read it triggers, so the line
  // says "installing…" until the command has exited, then "checking…".
  const running = (action: ActionHandle, r: Running): Running =>
    action.state === "running" ? r : probing ? "checking" : null;
  const failure = (action: ActionHandle) => (action.state === "failed" ? action.error : undefined);
  const statusOf = (step: Step): StepStatusView => {
    if (step === "trust-ca")
      return trustStatus(trustProbe, running(install, "installing"), failure(install));
    if (step === "set-proxy")
      return proxyStatus(proxyProbe, running(setProxyAction, "setting"), failure(setProxyAction));
    return trafficStatus(traffic, now);
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

          {onRecheck && <Recheck probing={probing} probedAt={probedAt} onRecheck={onRecheck} />}

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
                        install={install}
                        remedy={view.remedy}
                        onInstall={install.trigger}
                        save={saveCa}
                      />
                    ) : step === "set-proxy" ? (
                      <SetProxy
                        os={os}
                        addr={proxyAddr}
                        set={setProxyAction}
                        remedy={view.remedy}
                        onSetProxy={setProxyAction.trigger}
                        unset={unsetProxyAction}
                      />
                    ) : (
                      <MakeRequest
                        addr={proxyAddr}
                        remedy={view.remedy}
                        lastFlowAt={latest(traffic.lastDecryptedAt, traffic.lastPlainAt)}
                      />
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
