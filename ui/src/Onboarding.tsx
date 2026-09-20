import { useEffect, useState, type ReactNode } from "react";
import { useAction, type ActionHandle, type ActionState } from "./useAction";
import { COPY, type OnbEvent, type OnbState } from "./onboardingMachine";
import { UNREACHABLE, type ReachState } from "./api";
import {
  Button,
  ChoiceTiles,
  CopyText,
  Disclosure,
  Notice,
  Spinner,
  StatusLine,
  StepList,
  TaskStep,
  TimestampText,
  type NoticeTone,
} from "./components/index";
import {
  canAutoSetup,
  CA_FILENAME,
  deviceName,
  needsReach,
  probesRoute,
  probesTrust,
  platformChoices,
  reachTaskTitle,
  setupFor,
  splitProxyAddr,
  type Guide,
  type GuideStep,
  type HostOs,
  type PlatformId,
  type Task,
} from "./setup";
import {
  awaiting,
  foreignProxyNotice,
  NO_PROXY_READING,
  NO_TRAFFIC,
  proxyStatus,
  setUpOn,
  trafficReading,
  trafficStatus,
  trustStatus,
  type ProxyReading,
  type Running,
  type StepStatusView,
  type Traffic,
  type TrustProbe,
} from "./verdicts";

// The setup surface, built from round 4 of the design run of 2026-09-17. It
// leads with what this is, asks one question the user can answer, and answers
// it with numbered tasks: what to do, why, and the state something observed.
//
// A number is a reading aid; a state is a reading. No task reports itself done
// — every state comes from a probe of this machine, from the traffic layer, or
// from the reach this app itself holds — and a task whose precondition is not
// met is dimmed and still numbered, so the shape of the whole job is readable
// before any of it is done.
//
// DEC-33's two layers stay apart, and the platform axis sharpens the rule: a
// probe reads THIS machine, so it is read only where the chosen platform shares
// its stores. Everywhere else the task says when it will be confirmed rather
// than borrowing an answer about the wrong host.

const TITLE = "Set up capture";
const PURPOSE =
  "Trawler sits between a device and the internet. To show a device's HTTPS traffic, that device has to trust Trawler's certificate and send its traffic to Trawler.";
const QUESTION = "which device's traffic do you want to see";

// An action that resolves a string names what it produced; the note quotes that
// name rather than the one we asked for, since the two differ whenever the user
// renames the file in the save dialog.
type ActionNotes = Partial<Record<ActionState, ActionNote | ((named: string) => ActionNote)>>;

interface ActionNote {
  tone: NoticeTone;
  text: string;
}

// A prompt the user dismissed is not a failure the OS gets to word: it is the
// one outcome they chose, and the way back in is the same button or the steps
// below it (F18). The tool's own text is still payload, and stays reachable.
const PROMPT_CANCELLED = "The password prompt was cancelled. Try again, or follow the manual steps.";

const SETUP_NOTE: ActionNotes = {
  running: { tone: "pending", text: "setting up — approve the prompt…" },
  failed: { tone: "warn", text: "setting up failed — the steps below do it by hand." },
};

const SETPROXY_NOTE: ActionNotes = {
  running: { tone: "pending", text: "setting the proxy — approve the prompt…" },
  failed: { tone: "warn", text: "setting the proxy failed — the steps below do it by hand." },
};

const UNSETPROXY_NOTE: ActionNotes = {
  running: { tone: "pending", text: "turning the proxy off — approve the prompt…" },
  failed: { tone: "warn", text: "couldn't turn the proxy off — the steps below do it by hand." },
};

// Arming is the shell's dialog and then the bind. A yes needs no note — what is
// listening is the task's state, and it says so a line above. A no does: the
// dialog closes, the button comes back, and without this the screen says
// exactly what it says after a success.
const ARM_NOTE: ActionNotes = {
  running: { tone: "pending", text: "waiting for your answer…" },
  nothing: { tone: "mute", text: "not allowed — Trawler still answers on this machine only." },
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

const TICK_MS = 1000;

// Recency is judged against a clock that keeps moving while the screen is open,
// or a verdict true at mount would stay true forever. It lives inside each
// component that reads a traffic verdict and nowhere above them, so a tick
// re-renders those and not the instructions, the tiles or the guides, which no
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
  const refused = action.state === "failed" && action.failure?.refused === true;
  const entry = note[action.state];
  const shown = typeof entry === "function" ? entry(action.named) : entry;
  if (!shown && !refused) return null;
  return (
    <div data-testid={`${testId}-status`} data-state={action.state} className="pt-chrome-y">
      <Notice tone={refused ? "mute" : shown!.tone}>{refused ? PROMPT_CANCELLED : shown!.text}</Notice>
      {action.state === "failed" && action.failure !== undefined && (
        <div className="pt-2">
          <Disclosure summary="details">
            <p data-wire="command output" className="font-mono text-chrome text-ink-mute">
              {action.failure.detail}
            </p>
          </Disclosure>
        </div>
      )}
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

// A guide hides behind "do it manually" only where a hand is the second route:
// with no action above it these are the only instructions there are, and a
// summary promising an automatic alternative is a promise nothing keeps.
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
    <Disclosure summary="do it manually" defaultOpen={defaultOpen}>
      {body}
    </Disclosure>
  );
}

// The outcome reads where the request that produces it is made, which is the one
// thing DEC-13 accepts as proof that setup is over.
function ProvenOutcome({ device, traffic }: { device: string | null; traffic: Traffic }) {
  const now = useNow();
  if (trafficReading(traffic, now) !== "capturing") return null;
  return (
    <div data-testid="proven" className="px-4 pt-4">
      <Notice
        tone="ok"
        detail={
          <span>
            last request <TimestampText timestamp={traffic.lastDecryptedAt} format="relative" />
          </span>
        }
      >
        {setUpOn(device)}
      </Notice>
    </div>
  );
}

// The last task's state is the traffic layer's, and a clock is what keeps its
// reading honest while the screen sits open — so it is read here, in the one
// leaf a tick may re-render, rather than above the tasks that no clock can
// change. StatusLine renders the verdict and refuses the instruction that goes
// with it (its @avoid), so the remedy is the Notice beside it.
function TrafficState({ traffic }: { traffic: Traffic }) {
  const now = useNow();
  const status = trafficStatus(traffic, now);
  return (
    <>
      <StatusLine status={status} />
      {status.remedy !== undefined && (
        <div data-testid="traffic-remedy" className="pt-2">
          <Notice tone="mute">{status.remedy}</Notice>
        </div>
      )}
    </>
  );
}

// One reading of this machine governs both probed tasks, so one control re-takes
// it. The time stamps the reading: a probe is a snapshot, and nothing pushes
// when the user changes a setting elsewhere.
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

const addressLabel = (address: ReachState["interfaces"][number]) =>
  `${address.name} · ${address.address}`;

// The core answers this list best-first — the address the host routes from,
// IPv4 before IPv6, nothing that rotates above something that does not. So one
// address is the offer and the rest are a disclosure: a user typing a proxy
// into a phone by hand reads one line.
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
  const chosen = addresses.find((address) => address.address === value);
  const others = addresses.filter((address) => address.address !== value);
  return (
    <>
      <p className={ZONE_LABEL}>address to use</p>
      <p data-testid="listen-on" className="pt-2 font-ui text-body text-ink-body">
        {chosen && addressLabel(chosen)}
      </p>
      {others.length > 0 && (
        <div className="pt-2">
          <Disclosure summary="other addresses">
            <div className="flex flex-col items-start">
              {others.map((address) => (
                <Button
                  key={address.address}
                  frame="bare"
                  tone="mute"
                  onClick={() => onChange(address.address)}
                >
                  {addressLabel(address)}
                </Button>
              ))}
            </div>
          </Disclosure>
        </div>
      )}
    </>
  );
}

// The one task that changes this machine rather than the device the user picked.
// It states one consequence and nothing more: the full statement is in the
// confirm dialog the shell already shows, which is where a decision this size
// belongs (DEC-55).
function NetworkTask({
  n,
  title,
  reach,
  chosen,
  onChoose,
  arm,
  disarm,
}: {
  n: number;
  title: string;
  reach: ReachState;
  chosen: string | undefined;
  onChoose: (address: string) => void;
  arm: ActionHandle;
  disarm: ActionHandle;
}) {
  if (reach.armed && reach.proxy !== null) {
    const { host } = splitProxyAddr(reach.proxy);
    return (
      <TaskStep
        n={n}
        title={title}
        status={{
          glyph: "✓",
          tone: "ok",
          short: `allowed — listening on ${host} until Trawler quits`,
        }}
      >
        {disarm.trigger && (
          <Action
            testId="disarm-reach"
            label="stop listening"
            action={disarm}
            note={DISARM_NOTE}
            onClick={disarm.trigger}
            frame="bare"
          />
        )}
      </TaskStep>
    );
  }

  return (
    <TaskStep
      n={n}
      title={title}
      why="Trawler answers on this machine only until you do"
      status={{
        glyph: "○",
        tone: "muted",
        short: "not done — Trawler answers on this machine only",
      }}
    >
      {chosen === undefined ? (
        <Notice tone="mute">
          this machine holds no network address a second device could reach it at
        </Notice>
      ) : (
        <>
          <AddressChoice addresses={reach.interfaces} value={chosen} onChange={onChoose} />
          {arm.trigger && (
            <div className="pt-3">
              <Action
                testId="arm-reach"
                label="Allow devices on my Wi-Fi to connect"
                action={arm}
                note={ARM_NOTE}
                onClick={arm.trigger}
              />
            </div>
          )}
          <p className="pt-2 font-ui text-chrome text-ink-mute">
            other devices on your Wi-Fi will be able to use this proxy and see captured traffic
            until Trawler quits
          </p>
        </>
      )}
    </TaskStep>
  );
}

function ServerAndPort({ proxy }: { proxy: string }) {
  const { host, port } = splitProxyAddr(proxy);
  return (
    <div data-testid="device-address" className="flex gap-4">
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

// What belongs to the tasks above it rather than to any one of them — the
// action that does two of them at once, and the reversal that undoes them.
// It takes the task body's own left edge and leaves the number column empty,
// because a number here would read as a task nobody has to do.
function TaskAside({ children }: { children: ReactNode }) {
  return (
    <div className="flex gap-3 px-4 pt-3">
      <span aria-hidden="true" className="w-step-marker shrink-0" />
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

// Where one action covers two tasks, one disclosure covers both their guides:
// the manual route is the alternative to the button, so it holds everything the
// button would have done, in the order the tasks are numbered.
const manualSteps = (setup: {
  trust: Task;
  route: Task;
}): GuideStep[] => [...(setup.trust.guide?.steps ?? []), ...(setup.route.guide?.steps ?? [])];

const reversalSteps = (setup: { trust: Task; route: Task }): GuideStep[] => [
  ...(setup.trust.guide?.undo ?? []),
  ...(setup.route.guide?.undo ?? []),
];

// A task's instructions, and the state of the task they belong to. `dim` is the
// one thing a task with no instructions needs: it keeps its number and says
// which step comes first, rather than explaining what cannot happen yet (F9).
function GuidedTask({
  n,
  task,
  status,
  dim,
  guide,
  save,
  action,
  children,
}: {
  n: number;
  task: Task;
  status?: StepStatusView;
  dim?: boolean;
  guide?: Guide;
  save?: ActionHandle;
  action?: ReactNode;
  children?: ReactNode;
}) {
  const body = guide !== undefined || action !== undefined || children !== undefined;
  return (
    <TaskStep
      n={n}
      title={task.title}
      why={dim ? "after step 1" : task.why}
      status={dim ? undefined : status}
      dim={dim}
    >
      {!dim && body && (
        <>
          {children}
          {action}
          {guide && (
            <div className={action === undefined ? "" : "pt-3"}>
              <GuideView guide={guide} save={save} collapsed={action !== undefined} />
            </div>
          )}
        </>
      )}
    </TaskStep>
  );
}

export function Onboarding({
  state,
  dispatch,
  os = "other",
  proxyAddr = "127.0.0.1:7420",
  trustProbe = "unknown",
  proxy = NO_PROXY_READING,
  probing = false,
  probedAt = null,
  traffic = NO_TRAFFIC,
  reach = UNREACHABLE,
  onSetUpThisMachine,
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
  proxy?: ProxyReading;
  probing?: boolean;
  probedAt?: number | null;
  traffic?: Traffic;
  reach?: ReachState;
  onSetUpThisMachine?: () => Promise<null | void>;
  onSaveCa?: () => Promise<string | null>;
  onSetProxy?: () => Promise<null | void>;
  onUnsetProxy?: () => Promise<void>;
  onRecheck?: () => void;
  onArmReach?: (address: string) => Promise<null | void>;
  onDisarmReach?: () => Promise<void>;
}) {
  const [platform, setPlatform] = useState<PlatformId>("this");
  const [listenOn, setListenOn] = useState<string | null>(null);

  const setUp = useAction(onSetUpThisMachine);
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
  // Named only where this machine is the device: a decrypted request proves
  // capture and never says which device made it, and the one platform where
  // those are the same answer is the one the user is sitting at.
  const provenOn = platform === "this" ? deviceName("this", os) : null;

  const autoHost = canAutoSetup(os);
  // An action that changes this machine is offered for the store it changes,
  // and only where the chosen platform reads that store: installing here does
  // nothing for a Simulator with a keychain of its own, and setting this
  // machine's proxy is exactly what the Simulator routes through.
  const trustHere = probesTrust(platform) && autoHost;
  const routeHere = probesRoute(platform) && autoHost ? setProxyAction.trigger : undefined;
  // One prompt for the one thing most users want, and only where both halves of
  // it are this machine's to change (F27). Where only one half is — the
  // Simulator's proxy — that half stays the primary action on its own, and the
  // certificate half of this machine's setup has no button of its own at all:
  // what the one action replaced stays reachable in the manual route below it.
  const setUpHere = trustHere && routeHere ? setUp.trigger : undefined;

  // The action the user just took outranks the re-read it triggers, and the one
  // combined action outranks both halves it is running.
  const busy = (own: ActionHandle, r: Running): Running =>
    setUp.state === "running" || own.state === "running" ? r : probing ? "checking" : null;

  const trustState = probesTrust(platform)
    ? trustStatus(trustProbe, busy(setUp, "installing"), setup.confirmOn)
    : awaiting(setup.confirmOn);
  const routeState = probesRoute(platform)
    ? proxyStatus(proxy.status, busy(setProxyAction, "setting"), {
        device: deviceName("this", os),
        confirmOn: setup.confirmOn,
      })
    : awaiting(setup.confirmOn);

  // Off-machine, every task after the network one needs an address that does
  // not exist yet — so they keep their numbers and say which step comes first.
  const blocked = needsReach(platform) && !armed;
  const first = needsReach(platform) ? 1 : 0;

  return (
    <div className="flex h-full flex-col overflow-auto">
      <div className="mx-auto w-full max-w-dialog-max py-6">
        <div data-testid="lead" className="px-4">
          <p className="font-serif text-note text-ink">{TITLE}</p>
          <p className="pt-2 font-ui text-body text-ink-body">{PURPOSE}</p>
        </div>

        <div data-testid="platform" className="px-4 pt-4">
          <p className={ZONE_LABEL}>{QUESTION}</p>
          <div className="pt-2">
            <ChoiceTiles
              options={platformChoices(os)}
              value={platform}
              onChange={(id) => setPlatform(id as PlatformId)}
              ariaLabel={QUESTION}
            />
          </div>
        </div>

        <div data-testid="steps" className="pt-2">
          {needsReach(platform) && (
            <NetworkTask
              n={1}
              title={reachTaskTitle(platform, os)}
              reach={reach}
              chosen={chosen}
              onChoose={setListenOn}
              arm={arm}
              disarm={disarm}
            />
          )}

          <GuidedTask
            n={first + 1}
            task={setup.trust}
            status={trustState}
            dim={blocked}
            guide={setUpHere === undefined ? setup.trust.guide : undefined}
            save={onSaveCa ? saveCa : undefined}
          />

          <GuidedTask
            n={first + 2}
            task={setup.route}
            status={routeState}
            dim={blocked}
            guide={setUpHere === undefined ? setup.route.guide : undefined}
            action={
              setUpHere === undefined && routeHere !== undefined ? (
                <Action
                  testId="set-proxy"
                  label="Set the system proxy"
                  action={setProxyAction}
                  note={SETPROXY_NOTE}
                  onClick={routeHere}
                  disabled={probing}
                />
              ) : undefined
            }
          >
            {proxy.status === "foreign" && probesRoute(platform) && (
              <div data-testid="foreign-proxy" className="pb-2">
                <Notice tone="mute">
                  {foreignProxyNotice(deviceName("this", os), proxy.address)}
                </Notice>
              </div>
            )}
            {needsReach(platform) && armed && reach.proxy !== null && (
              <ServerAndPort proxy={reach.proxy} />
            )}
          </GuidedTask>

          {/* One primary action per screen, and the two single ones stay inside
              the manual route it replaced. It sits under the tasks it covers
              because it is both of them, and under the reversal rule DEC-13
              inherited: an undo is offered once the setup here is ours to undo,
              and still offered where a probe cannot say.

              The reversal names the half it performs. One action installs a
              certificate and sets a proxy; only the proxy can be put back, and
              DEC-13 never offered to untrust a store — so the control says
              what it does, and the line beside it says what stays and where
              the steps for it are. A button that claimed to undo the trust
              would be answering, falsely, a question about what this machine
              trusts. */}
          {setUpHere !== undefined && (
            <TaskAside>
              {proxy.status === "ours" && trustProbe === "trusted" ? (
                unsetProxyAction.trigger && (
                  <>
                    <Action
                      testId="unset-proxy"
                      label="stop routing through Trawler"
                      action={unsetProxyAction}
                      note={UNSETPROXY_NOTE}
                      onClick={unsetProxyAction.trigger}
                      frame="bare"
                    />
                    <p className="pt-2 font-ui text-chrome text-ink-mute">
                      Trawler's certificate stays trusted on {deviceName("this", os)} — the steps
                      below remove it.
                    </p>
                  </>
                )
              ) : (
                <>
                  <Action
                    testId="set-up"
                    label={`Set up ${deviceName("this", os)}`}
                    action={setUp}
                    note={SETUP_NOTE}
                    onClick={setUpHere}
                    disabled={probing}
                  />
                  <p className="pt-2 font-ui text-chrome text-ink-mute">
                    one password prompt — it installs the certificate and sets the proxy
                  </p>
                </>
              )}
              {/* One action, one manual alternative: what the button does in
                  one prompt, the steps do in two tasks' worth of commands. Two
                  disclosures under two tasks with no button above either would
                  promise an automatic route that is not there (F10). */}
              <div className="pt-3">
                <Disclosure summary="do it manually">
                  <GuideSteps steps={manualSteps(setup)} save={onSaveCa ? saveCa : undefined} />
                  {reversalSteps(setup).length > 0 && (
                    <>
                      <p className="pt-3 font-ui text-chrome text-ink-mute">to undo it later</p>
                      <div className="pt-2">
                        <GuideSteps steps={reversalSteps(setup)} />
                      </div>
                    </>
                  )}
                </Disclosure>
              </div>
            </TaskAside>
          )}

          {/* The reversal for the Simulator, whose primary action is the proxy
              alone: an action that changed this host may not become
              irreversible because a probe stopped answering. */}
          {setUpHere === undefined &&
            routeHere !== undefined &&
            unsetProxyAction.trigger &&
            (proxy.status === "ours" || setProxyAction.state === "done") && (
              <TaskAside>
                <Action
                  testId="unset-proxy"
                  label="stop routing through Trawler"
                  action={unsetProxyAction}
                  note={UNSETPROXY_NOTE}
                  onClick={unsetProxyAction.trigger}
                  frame="bare"
                />
              </TaskAside>
            )}

          <GuidedTask n={first + 3} task={setup.try} dim={blocked}>
            <TrafficState traffic={traffic} />
            {setup.tryCommand && (
              <>
                <p className="pt-2 font-ui text-chrome text-ink-mute">or run this in a terminal</p>
                <p className="pt-2 font-mono" data-wire="shell command">
                  <CopyText value={setup.tryCommand} label="the try-it command" />
                </p>
              </>
            )}
          </GuidedTask>
        </div>

        {(probesTrust(platform) || probesRoute(platform)) && onRecheck && (
          <ProbeRow probing={probing} probedAt={probedAt} onRecheck={onRecheck} />
        )}

        <ProvenOutcome device={provenOn} traffic={traffic} />

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
