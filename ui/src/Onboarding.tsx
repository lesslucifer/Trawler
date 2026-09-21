import { useEffect, useState, type ReactNode } from "react";
import { useAction, type ActionHandle, type ActionState } from "./useAction";
import { COPY, type OnbEvent, type OnbState } from "./onboardingMachine";
import { UNREACHABLE, type ReachState } from "./api";
import {
  Button,
  CommandBlock,
  CopyText,
  Disclosure,
  Notice,
  Select,
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
  combine,
  deviceName,
  isCommand,
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

// A value is read inside the sentence that names it, because that sentence is
// what says which field it goes in. A command is read somewhere else entirely,
// so it leaves the sentence and takes a surface of its own.
function Value({ value, label, wire }: { value: string; label: string; wire: string }) {
  return (
    <span data-wire={wire} className="font-mono">
      <CopyText value={value} label={label} />
    </span>
  );
}

function StepParts({ step, save }: { step: GuideStep; save?: ActionHandle }) {
  const said = step.filter((part) => !isCommand(part));
  const commands = step.filter(isCommand);
  return (
    <>
      {said.length > 0 && (
        <span className="flex flex-wrap items-center gap-chrome-gap">
          {said.map((part, i) => {
            if (typeof part === "string") return <span key={i}>{part}</span>;
            if ("copy" in part)
              return <Value key={i} value={part.copy} label={part.label} wire={part.wire} />;
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
      )}
      {commands.map((part, i) => (
        <CommandBlock key={i} value={part.command} label={part.label} />
      ))}
    </>
  );
}

// The save control produces the file the step names, so its outcome lands inside
// that step rather than under the guide that contains it.
function GuideSteps({ steps, save }: { steps: GuideStep[]; save?: ActionHandle }) {
  const carriesSave = (step: GuideStep) => step.some((p) => typeof p === "object" && "save" in p);
  return (
    <StepList
      steps={steps.map((step, i) => (
        <div key={i} className="flex min-w-0 flex-col gap-chrome-gap">
          <StepParts step={step} save={save} />
          {save && carriesSave(step) && <ActionNote testId="save-ca" action={save} note={SAVECA_NOTE} />}
        </div>
      ))}
    />
  );
}

// What a guide says it is before it is opened. Where a button does the job,
// the guide is the other way of doing it and says so. Where there is no
// button it is the only way, and a count is the one summary that promises no
// automatic alternative while still saying how much work is behind it (F10).
const MANUAL = "do it manually";
const stepCount = (guide: Guide) =>
  `${guide.steps.length} step${guide.steps.length === 1 ? "" : "s"}`;

// One task's instructions: the route, then the three things a numbered step
// must not be. Notes are facts about this platform and sit under the steps
// unnumbered; a second route and the way back are each behind a summary of
// their own, so the primary path is what the reader meets.
function GuideView({
  guide,
  save,
  summary,
  defaultOpen = false,
}: {
  guide: Guide;
  save?: ActionHandle;
  summary: string;
  defaultOpen?: boolean;
}) {
  const alternatives = guide.alternatives ?? [];
  const undo = guide.undo ?? [];
  return (
    <Disclosure summary={summary} defaultOpen={defaultOpen}>
      <GuideSteps steps={guide.steps} save={save} />
      {guide.notes?.map((note) => (
        <p key={note} data-testid="guide-note" className="pt-2 font-ui text-chrome text-ink-mute">
          {note}
        </p>
      ))}
      {alternatives.length > 0 && (
        <div className="pt-3">
          <Disclosure summary="other ways">
            <GuideSteps steps={alternatives} save={save} />
          </Disclosure>
        </div>
      )}
      {undo.length > 0 && (
        <div className="pt-3">
          <Disclosure summary={guide.undoTitle ?? "to undo it later"}>
            <GuideSteps steps={undo} />
          </Disclosure>
        </div>
      )}
    </Disclosure>
  );
}

// The outcome reads where the request that produces it is made, which is the one
// thing DEC-13 accepts as proof that setup is over.
function ProvenOutcome({ device, traffic }: { device: string | null; traffic: Traffic }) {
  const now = useNow();
  if (trafficReading(traffic, now) !== "capturing") return null;
  return (
    <div data-testid="proven" className="px-4">
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
      className="flex items-center justify-end gap-chrome-gap px-4 font-ui text-control text-ink-mute"
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

// A task's instructions, and the state of the task they belong to. `dim` is the
// one thing a task with no instructions needs: it keeps its number and says
// which step comes first, rather than explaining what cannot happen yet (F9).
function GuidedTask({
  n,
  task,
  status,
  dim,
  guide,
  guideOpen = false,
  save,
  action,
  children,
}: {
  n: number;
  task: Task;
  status?: StepStatusView;
  dim?: boolean;
  guide?: Guide;
  guideOpen?: boolean;
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
              <GuideView
                guide={guide}
                save={save}
                summary={action === undefined ? stepCount(guide) : MANUAL}
                defaultOpen={guideOpen}
              />
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

  // Each task's own guide survives only where no single action replaces both of
  // them; where one does, the two are merged into its alternative below.
  const trustGuide = setUpHere === undefined ? setup.trust.guide : undefined;
  const routeGuide = setUpHere === undefined ? setup.route.guide : undefined;
  const routeAction =
    setUpHere === undefined && routeHere !== undefined ? (
      <Action
        testId="set-proxy"
        label="Set the system proxy"
        action={setProxyAction}
        note={SETPROXY_NOTE}
        onClick={routeHere}
        disabled={probing}
      />
    ) : undefined;

  // One guide is open, and it is the first whose task is not done: everything
  // above it is finished and everything below it waits on this one. A guide
  // under a button is never the open one — the button is the route, and the
  // steps are what a reader asks for. Nothing observes a task on another
  // device, so off-machine the first guided task stays open until a decrypted
  // request says the whole job is over — the one reading that settles it for
  // every platform at once.
  //
  // The second arm cannot fire today, and is kept because the rule is what is
  // written here rather than what the current probes happen to allow: the two
  // guides are always both present or both absent, so the route's turn needs a
  // trust task that reads done while its own steps still render — and the only
  // host that renders them is one with no one-click path, where the trust probe
  // answers for no store at all. It goes live the day a probe does, which is an
  // open row against the control API and not a change to this rule.
  const everDecrypted = traffic.lastDecryptedAt !== null;
  const guidesOpen = !blocked && !everDecrypted;
  const trustGuideOpen = guidesOpen && trustGuide !== undefined && trustState.tone !== "ok";
  const routeGuideOpen =
    guidesOpen &&
    !trustGuideOpen &&
    routeGuide !== undefined &&
    routeAction === undefined &&
    routeState.tone !== "ok";

  return (
    <div className="flex h-full flex-col overflow-auto">
      <div className="mx-auto flex w-full max-w-reading flex-col gap-6 py-8">
        <div data-testid="lead" className="px-4">
          <p className="font-serif text-note text-ink">{TITLE}</p>
          <p className="pt-2 font-ui text-body text-ink-body">{PURPOSE}</p>
        </div>

        <div data-testid="platform" className="flex flex-wrap items-center gap-3 px-4">
          <p className={ZONE_LABEL}>{QUESTION}</p>
          <Select
            options={platformChoices(os)}
            value={platform}
            onChange={(id) => setPlatform(id as PlatformId)}
            label={QUESTION}
            searchable={false}
            className="w-picker"
          />
        </div>

        <div data-testid="steps" className="flex flex-col gap-4">
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
            guide={trustGuide}
            guideOpen={trustGuideOpen}
            save={onSaveCa ? saveCa : undefined}
          />

          <GuidedTask
            n={first + 2}
            task={setup.route}
            status={routeState}
            dim={blocked}
            guide={routeGuide}
            guideOpen={routeGuideOpen}
            action={routeAction}
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
              because it is both of them.

              The action and its reversal are two controls, not two halves of
              one: DEC-13's inherited rule is that work which changed this host
              stays reversible from the moment it ran, so the way out appears
              as soon as the action resolved and does not wait for the re-read
              it triggered — a probe that has not answered, or never will, may
              not strand a machine this app just changed. The action itself
              stays until the probes say the work is done, because what is
              done is still theirs to say and never the click's.

              Whichever of the two opened the way out — the probe reading a
              proxy that is ours, or the action having just resolved — a
              reversal that SUCCEEDED closes it. That is proof the thing it
              offered to undo is undone, and the only proof that does not wait
              on a probe: the re-read the reversal triggers lands an IPC
              round-trip later, and until it does the old reading still says
              ours. A reversal that FAILED leaves the way out open, because
              the proxy it could not turn off is still set.

              The reversal names the half it performs. One action installs a
              certificate and sets a proxy; only the proxy can be put back, and
              DEC-13 never offered to untrust a store — so the control says
              what it does, and the line beside it says where the steps for the
              rest are. A button that claimed to undo the trust would be
              answering, falsely, a question about what this machine trusts. */}
          {setUpHere !== undefined && (
            <TaskAside>
              {!(proxy.status === "ours" && trustProbe === "trusted") && (
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
              {(proxy.status === "ours" || setUp.state === "done") &&
                unsetProxyAction.state !== "done" &&
                unsetProxyAction.trigger && (
                  <div className="pt-3">
                    <Action
                      testId="unset-proxy"
                      label="stop routing through Trawler"
                      action={unsetProxyAction}
                      note={UNSETPROXY_NOTE}
                      onClick={unsetProxyAction.trigger}
                      frame="bare"
                    />
                    <p className="pt-2 font-ui text-chrome text-ink-mute">
                      this turns the proxy off and nothing else — the steps below remove Trawler's
                      certificate.
                    </p>
                  </div>
                )}
              {/* One action, one manual alternative: what the button does in
                  one prompt, the steps do in two tasks' worth of commands. Two
                  disclosures under two tasks with no button above either would
                  promise an automatic route that is not there (F10). */}
              <div className="pt-3">
                <GuideView
                  guide={combine(setup.trust.guide, setup.route.guide)}
                  save={onSaveCa ? saveCa : undefined}
                  summary={MANUAL}
                />
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
                <div className="pt-2">
                  <CommandBlock value={setup.tryCommand} label="the try-it command" />
                </div>
              </>
            )}
          </GuidedTask>
        </div>

        {(probesTrust(platform) || probesRoute(platform)) && onRecheck && (
          <ProbeRow probing={probing} probedAt={probedAt} onRecheck={onRecheck} />
        )}

        <ProvenOutcome device={provenOn} traffic={traffic} />

        {state.everProven && (
          <div data-testid="back" className="px-4">
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
