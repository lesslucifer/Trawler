import { useEffect, useState, type ReactNode } from "react";
import { COPY, type OnbEvent, type OnbState } from "./onboardingMachine";
import { UNREACHABLE, type ReachState } from "./api";
import {
  Button,
  CopyText,
  Disclosure,
  Notice,
  Section,
  SegmentedControl,
  Select,
  Spinner,
  StatusLine,
  StepList,
  TimestampText,
  type NoticeTone,
} from "./components/index";
import {
  canAutoSetup,
  CA_FILENAME,
  needsReach,
  probesRoute,
  probesTrust,
  platformChoices,
  setupFor,
  splitProxyAddr,
  type Guide,
  type GuideStep,
  type HostOs,
  type PlatformId,
} from "./setup";
import {
  NO_TRAFFIC,
  proxyStatus,
  screenVerdict,
  trafficReading,
  trafficStatus,
  trustStatus,
  UNPROBEABLE_TRUST,
  type ProxyProbe,
  type Running,
  type ScreenVerdict,
  type StepStatusView,
  type Traffic,
  type TrustProbe,
} from "./verdicts";

// The setup surface, rebuilt from the design run of 2026-09-17. One question —
// what are you setting up — and everything below it belongs to that answer.
// Not a wizard: nothing observes per-step progress, so the screen draws no
// cursor and claims no position in a sequence.
//
// DEC-33's two layers stay apart, and the platform axis sharpens the rule: a
// probe reads THIS machine, so it is rendered only where the chosen platform is
// this machine or shares its stores. Everywhere else the screen says it cannot
// check rather than borrowing an answer about the wrong host.

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
  running: { tone: "pending", text: "installing — approve the system prompt…" },
  done: { tone: "ok", text: "installed. we'll confirm it on your first flow." },
  failed: { tone: "warn", text: "automatic install failed — the steps below do it by hand." },
};

const SETPROXY_NOTE: ActionNotes = {
  running: { tone: "pending", text: "setting the proxy — approve the prompt if asked…" },
  done: { tone: "ok", text: "proxy set. we'll confirm it on your first flow." },
  failed: { tone: "warn", text: "automatic setup failed — the steps below do it by hand." },
};

const UNSETPROXY_NOTE: ActionNotes = {
  running: { tone: "pending", text: "turning the proxy off — approve the prompt if asked…" },
  done: { tone: "ok", text: "proxy off. set it again to resume routing through Trawler." },
  failed: { tone: "warn", text: "couldn't turn the proxy off — the steps below do it by hand." },
};

// Arming is the shell's dialog and then the bind. A cancelled dialog resolves
// without arming, which is why there is no "done" note: what is listening is
// the verdict's business, and only the verdict can tell the two apart.
const ARM_NOTE: ActionNotes = {
  running: { tone: "pending", text: "waiting for your answer…" },
  failed: { tone: "warn", text: "couldn't start listening on that address." },
};

const DISARM_NOTE: ActionNotes = {
  running: { tone: "pending", text: "stopping…" },
  failed: { tone: "warn", text: "couldn't stop listening." },
};

// The save resolves a full path natively and a bare name in the browser; the
// steps below name a file, so the note does too.
const fileName = (path: string) => path.split(/[/\\]/).pop() ?? path;

const SAVECA_NOTE: ActionNotes = {
  running: { tone: "pending", text: "saving…" },
  done: (path: string) => ({
    tone: "ok",
    text:
      fileName(path) === CA_FILENAME
        ? `saved as ${CA_FILENAME}. now trust it with the steps here.`
        : `saved as ${fileName(path)} — the steps here name ${CA_FILENAME}, so use ${fileName(path)} instead.`,
  }),
  nothing: { tone: "mute", text: "nothing was saved — the save was cancelled." },
  failed: { tone: "warn", text: "couldn't save the certificate — check that Trawler is running." },
};

// SegmentedControl refuses more than four options and Select refuses fewer than
// five; a single address is no choice at all, so the action names it instead.
const SEGMENT_MAX = 4;

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
// or a verdict true at mount would stay true forever. It lives inside each
// component that reads a traffic verdict and nowhere above them, so a tick
// re-renders those and not the instructions, the picker or the guides, which no
// clock can change.
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
    <div data-testid={`${testId}-status`} data-state={action.state} className="pt-chrome-y">
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
  frame,
  disabled = false,
}: {
  testId: string;
  label: string;
  action: ActionHandle;
  note: ActionNotes;
  onClick: () => void;
  frame?: "bare";
  disabled?: boolean;
}) {
  return (
    <div>
      <Button
        onClick={onClick}
        frame={frame}
        tone={frame === "bare" ? "mute" : undefined}
        disabled={disabled || action.state === "running"}
      >
        {label}
      </Button>
      <ActionNote testId={testId} action={action} note={note} />
    </div>
  );
}

const ZONE_LABEL = "font-ui text-chrome uppercase tracking-label text-ink-mute";

function Command({ value, label }: { value: string; label: string }) {
  return (
    <span data-wire="shell command" className="font-mono">
      <CopyText value={value} label={label} />
    </span>
  );
}

function StepParts({ step, save }: { step: GuideStep; save?: ActionHandle }) {
  return (
    <span className="flex flex-wrap items-center gap-chrome-gap">
      {step.map((part, i) => {
        if (typeof part === "string") return <span key={i}>{part}</span>;
        if ("copy" in part) return <Command key={i} value={part.copy} label={part.label} />;
        return save?.trigger ? (
          <Button
            key={i}
            frame="bare"
            tone="mute"
            onClick={save.trigger}
            disabled={save.state === "running"}
          >
            save…
          </Button>
        ) : null;
      })}
    </span>
  );
}

// The save control produces the file the step names, so its outcome lands inside
// that step rather than under the guide that contains it.
function GuideSteps({ steps, save }: { steps: GuideStep[]; save?: ActionHandle }) {
  const carriesSave = (step: GuideStep) => step.some((p) => typeof p === "object" && "save" in p);
  return (
    <StepList
      steps={steps.map((step, i) => (
        <span key={i}>
          <StepParts step={step} save={save} />
          {save && carriesSave(step) && <ActionNote testId="save-ca" action={save} note={SAVECA_NOTE} />}
        </span>
      ))}
    />
  );
}

// A guide hides behind "set it up by hand" only where a hand is the second
// route: with no action above it these are the only instructions there are, and
// a summary promising an automatic alternative is a promise nothing keeps.
function GuideView({
  guide,
  save,
  collapsed = true,
  defaultOpen = false,
}: {
  guide: Guide;
  save?: ActionHandle;
  collapsed?: boolean;
  defaultOpen?: boolean;
}) {
  const body = (
    <>
      <GuideSteps steps={guide.steps} save={save} />
      {guide.undo && (
        <>
          <p className="pt-3 font-ui text-chrome text-ink-mute">{guide.undoTitle}</p>
          <div className="pt-2">
            <GuideSteps steps={guide.undo} />
          </div>
        </>
      )}
    </>
  );

  if (!collapsed) return body;
  return (
    <Disclosure summary="set it up by hand" defaultOpen={defaultOpen}>
      {body}
    </Disclosure>
  );
}

const VERDICT: Record<
  Exclude<ScreenVerdict, "capturing">,
  { tone: NoticeTone; text: string; detail: string }
> = {
  checking: {
    tone: "pending",
    text: "checking what is already set up",
    detail: "reading this machine's certificate store and proxy settings",
  },
  "just-arrived": {
    tone: "pending",
    text: "a connection just arrived",
    detail: "waiting to see whether it decrypts",
  },
  "not-decrypting": {
    tone: "warn",
    text: "traffic is arriving, but nothing is decrypting",
    detail: "the trust step below says what to fix, and for which platform",
  },
  "plain-only": {
    tone: "mute",
    text: "traffic is arriving over plain HTTP only",
    detail: "an HTTPS request is what proves decryption",
  },
  idle: {
    tone: "mute",
    text: "nothing is arriving now",
    detail: "everything below is still set up — make a request to prove it",
  },
  waiting: {
    tone: "mute",
    text: "nothing has reached Trawler yet",
    detail: "finish the steps below, then make any request",
  },
};

// The screen's lead sentence, and the one element on it that a clock can change.
function LeadVerdict({ phase, traffic }: { phase: OnbState["phase"]; traffic: Traffic }) {
  const verdict = screenVerdict(phase, traffic, useNow());
  if (verdict === "capturing") return null;
  const { tone, text, detail } = VERDICT[verdict];
  const lastFlow = traffic.lastDecryptedAt ?? traffic.lastPlainAt;
  return (
    <div data-testid="lead-verdict" data-verdict={verdict}>
      <Notice
        tone={tone}
        detail={
          verdict === "idle" && lastFlow !== null ? (
            <span>
              last flow <TimestampText timestamp={lastFlow} format="relative" />
            </span>
          ) : (
            detail
          )
        }
      >
        {text}
      </Notice>
    </div>
  );
}

// The outcome reads where the request that produces it is made, which is the one
// thing DEC-13 accepts as proof that setup is over.
function ProvenOutcome({ phase, traffic }: { phase: OnbState["phase"]; traffic: Traffic }) {
  const now = useNow();
  if (screenVerdict(phase, traffic, now) !== "capturing") return null;
  return (
    <div data-testid="proven" className="pt-3">
      <Notice
        tone="ok"
        detail={
          <span>
            last flow <TimestampText timestamp={traffic.lastDecryptedAt} format="relative" />
          </span>
        }
      >
        Trawler is capturing
      </Notice>
    </div>
  );
}

// StatusLine renders the verdict and refuses the instruction that goes with it
// (its @avoid: "a verdict plus an instruction is two elements — pass
// StepStatusView.remedy to a Notice"). This is that Notice, so a short verdict
// never costs the guidance the resolver computed for it.
function Verdict({
  testId,
  status,
  stated,
}: {
  testId: string;
  status: StepStatusView;
  // What the action's own note is already saying. A failed action's remedy IS
  // its error, and one failure stated twice reads as two.
  stated?: string;
}) {
  const remedy = status.remedy === stated ? undefined : status.remedy;
  return (
    <>
      <StatusLine status={status} />
      {remedy !== undefined && (
        <div data-testid={`${testId}-guidance`} className="pt-2">
          <Notice tone="mute">{remedy}</Notice>
        </div>
      )}
    </>
  );
}

// The remedy the arriving verdict owes: a verdict plus an instruction is two
// elements, and the instruction belongs beside the precondition it instructs
// rather than in the headline three zones above it. It stays mute — the verdict
// already carries the alarm, and one failure raising two warn glyphs is two
// alarms for one fact.
function ArrivingRemedy({ traffic }: { traffic: Traffic }) {
  const now = useNow();
  if (trafficReading(traffic, now) !== "not-decrypting") return null;
  return (
    <div data-testid="trust-remedy" className="pt-2">
      <Notice tone="mute">{trafficStatus(traffic, now).remedy}</Notice>
    </div>
  );
}

// One reading of this machine governs both probed preconditions, so one control
// re-takes it. The time stamps the reading: a probe is a snapshot, and nothing
// pushes when the user changes a setting elsewhere.
function ProbeRow({
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
      data-testid="probe"
      className="flex items-center justify-end gap-chrome-gap px-4 pt-3 font-ui text-control text-ink-mute"
    >
      {probing ? (
        <span className="flex items-center gap-chrome-gap">
          <Spinner /> checking this machine…
        </span>
      ) : (
        probedAt !== null && (
          <span>
            checked <TimestampText timestamp={probedAt} format="relative" />
          </span>
        )
      )}
      <Button onClick={onRecheck} frame="bare" tone="mute" disabled={probing}>
        check again
      </Button>
    </div>
  );
}

function Precondition({
  testId,
  title,
  children,
}: {
  testId: string;
  title: string;
  children: ReactNode;
}) {
  return (
    <div data-testid={`precondition-${testId}`}>
      <Section title={title}>{children}</Section>
    </div>
  );
}

function AddressChoice({
  addresses,
  value,
  onChange,
}: {
  addresses: ReachState["interfaces"];
  value: string;
  onChange: (address: string) => void;
}) {
  if (addresses.length < 2) return null;
  const options = addresses.map((i) => ({
    value: i.address,
    label: `${i.name} · ${i.address}`,
  }));
  return (
    <>
      <p className={`pt-3 ${ZONE_LABEL}`}>listen on</p>
      <div className="pt-2">
        {addresses.length <= SEGMENT_MAX ? (
          <SegmentedControl
            options={options}
            value={value}
            onChange={onChange}
            ariaLabel="address to listen on"
          />
        ) : (
          <Select
            options={options.map((o) => ({ id: o.value, label: o.label }))}
            value={value}
            onChange={onChange}
            label="address to listen on"
          />
        )}
      </div>
    </>
  );
}

function ReachPrecondition({
  reach,
  chosen,
  onChoose,
  arm,
  disarm,
}: {
  reach: ReachState;
  chosen: string | undefined;
  onChoose: (address: string) => void;
  arm: ActionHandle;
  disarm: ActionHandle;
}) {
  if (reach.armed && reach.proxy !== null) {
    const { host } = splitProxyAddr(reach.proxy);
    return (
      <Precondition testId="reach" title="let this device reach Trawler">
        <StatusLine
          status={{ glyph: "✓", tone: "ok", short: `listening on ${host} — until Trawler quits` }}
        />
        {disarm.trigger && (
          <div className="pt-2">
            <Action
              testId="disarm-reach"
              label="stop listening"
              action={disarm}
              note={DISARM_NOTE}
              onClick={disarm.trigger}
              frame="bare"
            />
          </div>
        )}
      </Precondition>
    );
  }

  return (
    <Precondition testId="reach" title="let this device reach Trawler">
      <StatusLine
        status={{ glyph: "○", tone: "muted", short: "Trawler answers on this machine only" }}
      />
      {chosen === undefined ? (
        <div className="pt-2">
          <Notice tone="mute">
            this machine holds no network address a second device could reach it at
          </Notice>
        </div>
      ) : (
        <>
          <AddressChoice addresses={reach.interfaces} value={chosen} onChange={onChoose} />
          {arm.trigger && (
            <div className="pt-3">
              <Action
                testId="arm-reach"
                label={
                  reach.interfaces.length === 1
                    ? `let devices reach Trawler at ${chosen}`
                    : "let devices reach Trawler"
                }
                action={arm}
                note={ARM_NOTE}
                onClick={arm.trigger}
              />
            </div>
          )}
          <div className="pt-3">
            <Notice
              tone="mute"
              detail="Anything on that network will be able to route through the proxy and to read the flows Trawler has captured. Trawler stops listening when you turn this off, and whenever it quits."
            >
              Trawler will ask before it starts listening
            </Notice>
          </div>
        </>
      )}
    </Precondition>
  );
}

function ServerAndPort({ proxy }: { proxy: string }) {
  const { host, port } = splitProxyAddr(proxy);
  return (
    <div data-testid="device-address" className="flex gap-4 pt-2">
      <div>
        <p className={ZONE_LABEL}>server</p>
        <p className="pt-2 font-mono" data-wire="proxy host">
          <CopyText value={host} label="the proxy host" />
        </p>
      </div>
      <div>
        <p className={ZONE_LABEL}>port</p>
        <p className="pt-2 font-mono" data-wire="proxy port">
          <CopyText value={String(port)} label="the proxy port" />
        </p>
      </div>
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
  probing = false,
  probedAt = null,
  traffic = NO_TRAFFIC,
  reach = UNREACHABLE,
  onInstallCa,
  onSaveCa,
  onSetProxy,
  onUnsetProxy,
  onRecheck,
  onArmReach,
  onDisarmReach,
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
  reach?: ReachState;
  onInstallCa?: () => Promise<void>;
  onSaveCa?: () => Promise<string | null>;
  onSetProxy?: () => Promise<void>;
  onUnsetProxy?: () => Promise<void>;
  onRecheck?: () => void;
  onArmReach?: (address: string) => Promise<void>;
  onDisarmReach?: () => Promise<void>;
}) {
  const [platform, setPlatform] = useState<PlatformId>("this");
  const [listenOn, setListenOn] = useState<string | null>(null);

  const install = useAction(onInstallCa);
  const saveCa = useAction(onSaveCa);
  const setProxyAction = useAction(onSetProxy);
  const unsetProxyAction = useAction(onUnsetProxy);
  const chosen =
    reach.interfaces.find((i) => i.address === listenOn)?.address ?? reach.interfaces[0]?.address;
  const arm = useAction(
    onArmReach && chosen !== undefined ? () => onArmReach(chosen) : undefined,
  );
  const disarm = useAction(onDisarmReach);

  const armed = reach.armed && reach.proxy !== null && reach.ca_cert_url !== null;
  const setup = setupFor(platform, {
    os,
    proxyAddr,
    reach: armed ? { proxy: reach.proxy!, caCertUrl: reach.ca_cert_url! } : null,
  });

  const autoHost = canAutoSetup(os);
  // An action that changes this machine is offered for the store it changes,
  // and only where the chosen platform reads that store: installing here does
  // nothing for a Simulator with a keychain of its own, and setting this
  // machine's proxy is exactly what the Simulator routes through.
  // One value per precondition: the automatic action it offers, or nothing. The
  // guide below reads the same value, so "collapsed" and "there is a button"
  // cannot drift apart.
  const installHere = probesTrust(platform) && autoHost ? install.trigger : undefined;
  const routeHere = probesRoute(platform) && autoHost ? setProxyAction.trigger : undefined;

  // The action the user just took outranks the re-read it triggers, so the line
  // says "installing…" until the command has exited, then "checking…".
  const running = (action: ActionHandle, r: Running): Running =>
    action.state === "running" ? r : probing ? "checking" : null;
  const failure = (action: ActionHandle) => (action.state === "failed" ? action.error : undefined);

  const trustVerdict = probesTrust(platform)
    ? trustStatus(trustProbe, running(install, "installing"), failure(install))
    : UNPROBEABLE_TRUST;
  const routeVerdict = proxyStatus(
    proxyProbe,
    running(setProxyAction, "setting"),
    failure(setProxyAction),
  );

  return (
    <div className="flex h-full flex-col overflow-auto">
      <div className="mx-auto w-full max-w-dialog-max py-6">
        <div className="px-4">
          <LeadVerdict phase={state.phase} traffic={traffic} />
        </div>

        <div data-testid="platform" className="px-4 pt-4">
          <p className={ZONE_LABEL}>what are you setting up</p>
          <div className="pt-2">
            <Select
              options={platformChoices(os)}
              value={platform}
              onChange={(id) => setPlatform(id as PlatformId)}
              label="platform"
            />
          </div>
        </div>

        {(probesTrust(platform) || probesRoute(platform)) && onRecheck && (
          <ProbeRow probing={probing} probedAt={probedAt} onRecheck={onRecheck} />
        )}

        <div data-testid="preconditions" className="pt-3">
          <Section variant="band" title="what has to be true">
            {needsReach(platform) && (
              <ReachPrecondition
                reach={reach}
                chosen={chosen}
                onChoose={setListenOn}
                arm={arm}
                disarm={disarm}
              />
            )}

            <Precondition testId="trust" title="trust Trawler's certificate">
              <Verdict testId="trust" status={trustVerdict} stated={failure(install)} />
              <ArrivingRemedy traffic={traffic} />
              {installHere && (
                <div className="pt-2">
                  <Action
                    testId="install-ca"
                    label="install certificate"
                    action={install}
                    note={INSTALL_NOTE}
                    onClick={installHere}
                    disabled={probing}
                  />
                </div>
              )}
              {setup?.trustNote && (
                <div className="pt-2">
                  <Notice tone="mute">{setup.trustNote}</Notice>
                </div>
              )}
              <div className="pt-3">
                {setup?.trust ? (
                  <GuideView
                    guide={setup.trust}
                    save={onSaveCa ? saveCa : undefined}
                    collapsed={installHere !== undefined}
                    defaultOpen={install.state === "failed"}
                  />
                ) : (
                  setup === null && (
                    <Notice tone="mute">
                      the certificate route opens once Trawler is reachable — the step above opens it
                    </Notice>
                  )
                )}
              </div>
            </Precondition>

            <Precondition testId="route" title="route its traffic through Trawler">
              {probesRoute(platform) && (
                <Verdict testId="route" status={routeVerdict} stated={failure(setProxyAction)} />
              )}
              {routeHere && (
                <div className="pt-2">
                  <Action
                    testId="set-proxy"
                    label="set system proxy"
                    action={setProxyAction}
                    note={SETPROXY_NOTE}
                    onClick={routeHere}
                    disabled={probing}
                  />
                </div>
              )}
              {/* DEC-13's inherited assertion, both halves: the reversal is
                  offered only once a proxy here is ours to turn off — and it is
                  still offered where the probe cannot say, because an action that
                  changed this host may not become irreversible when a probe
                  fails. */}
              {routeHere &&
                unsetProxyAction.trigger &&
                (proxyProbe === "ours" || setProxyAction.state === "done") && (
                <div className="pt-2">
                  <Action
                    testId="unset-proxy"
                    label="stop routing through Trawler"
                    action={unsetProxyAction}
                    note={UNSETPROXY_NOTE}
                    onClick={unsetProxyAction.trigger}
                    frame="bare"
                  />
                </div>
                )}
              {setup?.routeNote && (
                <div className="pt-2">
                  <Notice tone="mute">{setup.routeNote}</Notice>
                </div>
              )}
              {needsReach(platform) && armed && reach.proxy !== null && (
                <ServerAndPort proxy={reach.proxy} />
              )}
              <div className="pt-3">
                {setup?.route ? (
                  <GuideView
                    guide={setup.route}
                    collapsed={routeHere !== undefined}
                    defaultOpen={setProxyAction.state === "failed"}
                  />
                ) : (
                  setup === null && (
                    <Notice tone="mute">
                      there is no address to point the device at yet — the reach step above makes one
                    </Notice>
                  )
                )}
              </div>
            </Precondition>
          </Section>
        </div>

        <div data-testid="try" className="px-4 pt-4">
          {setup === null ? (
            <Notice tone="mute">
              nothing to try until Trawler is reachable and the device is pointed at it
            </Notice>
          ) : setup.tryCommand ? (
            <>
              <p className="font-ui text-chrome text-ink-mute">{setup.tryIt}</p>
              <p className="pt-2 font-mono" data-wire="shell command">
                <CopyText value={setup.tryCommand} label="the try-it command" />
              </p>
            </>
          ) : (
            <Notice tone="mute">{setup.tryIt}</Notice>
          )}
          <ProvenOutcome phase={state.phase} traffic={traffic} />
        </div>

        {state.everProven && (
          <div data-testid="back" className="px-4 pt-4">
            <Button onClick={() => dispatch({ t: "exit" })} frame="bare" tone="mute">
              <span aria-hidden="true">‹ </span>
              {COPY.back}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
