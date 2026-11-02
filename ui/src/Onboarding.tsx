import { useEffect, useState, type ReactNode } from "react";
import {
  markedUrl,
  REQUEST_HOST,
  type AnotherDevice,
  type RequestRoute,
} from "./ownRequest";
import { useAction, type ActionHandle, type ActionState } from "./useAction";
import {
  Explained,
  GuideSteps as SharedGuideSteps,
  StepParts,
  type StepControls,
} from "./manualSteps";
import {
  COPY,
  opensOn,
  type OnbEvent,
  type OnbState,
} from "./onboardingMachine";
import { UNREACHABLE, type ReachState } from "./api";
import {
  Button,
  CopyText,
  Disclosure,
  Help,
  Notice,
  Select,
  StatusLine,
  TaskStep,
  TimestampText,
  type NoticeTone,
} from "./components/index";
import {
  canAutoSetup,
  CA_FILES,
  caFormatFor,
  checkStep,
  confirmHttpsOn,
  deviceName,
  lostAddress,
  needsReach,
  probesOwnStore,
  probesRoute,
  probesTrust,
  platformChoices,
  reachTaskTitle,
  readsOverAdb,
  scansCodes,
  marksItsOwn,
  setupFor,
  sharesHostWire,
  splitProxyAddr,
  type CaFormat,
  type Guide,
  type GuideStep,
  type HostOs,
  type PlatformId,
  type PlatformSetup as PlatformGuides,
  type RouteName,
  type SavedCa,
  type Task,
} from "./setup";
import {
  addressRefusedNow,
  addressTrustStatus,
  byOwnRequest,
  deviceRouteStatus,
  deviceTrustStatus,
  emulatorRefusal,
  emulatorRouteStatus,
  emulatorsRouted,
  emulatorTrustStatus,
  evidenceOf,
  foreignProxyNotice,
  markedDevice,
  mixedTryStatus,
  NO_PROXY_READING,
  NO_TRAFFIC,
  notDecryptingHere,
  provenDevice,
  proxyStatus,
  refusedAddress,
  routeByTraffic,
  settledRefusal,
  setUpOn,
  storesTrusted,
  simulatorTrustStatus,
  removeCaNote,
  stopRoutingNote,
  trafficReading,
  trafficStatus,
  trustedEmulators,
  trustedSimulators,
  SYSTEM_TOOLS_TRUSTED,
  trustStatus,
  TUNNEL_GRACE_MS,
  verdict,
  type AndroidSdk,
  type EmulatorReading,
  type ProxyReading,
  type Running,
  type SimulatorTrust,
  type StepStatusView,
  type AddressRefusal,
  type DeviceReading,
  type Traffic,
  type TrustProbe,
} from "./verdicts";

// The setup surface, built from the design run of 2026-09-17 as round 7 left
// it. It leads with what this is, asks one question the user can answer, and answers
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
const UNANSWERED = "choose a device";

// An action that resolves a string names what it produced; the note quotes that
// name rather than the one we asked for, since the two differ whenever the user
// renames the file in the save dialog.
type ActionNotes = Partial<
  Record<ActionState, ActionNote | ((named: string) => ActionNote)>
> & { refused?: string };

interface ActionNote {
  tone: NoticeTone;
  text: string;
}

// A prompt the user dismissed is not a failure the OS gets to word: it is the
// one outcome they chose, and the way back in is the same button or the steps
// below it (F18). The tool's own text is still payload, and stays reachable.
// Each host names its own prompt: macOS asks for a password, Windows for an
// approval (W12's W2).
const PROMPT_CANCELLED =
  "the password prompt was cancelled. Try again, or do it by hand.";
const APPROVAL_CANCELLED =
  "the approval was cancelled. Try again, or do it by hand.";

// An approval the OS could not show never reached the user, so it is not theirs
// to have cancelled (-60007); the task's state line says what it left undone.
const PROMPT_UNSHOWABLE =
  "macOS couldn't show the prompt to approve this, so it isn't done — do it by hand below.";

// Declining the replace-a-foreign-proxy confirm resolves nothing rather than
// rejecting, so it is not a failure and may not be worded as one — but it is
// still an act, and an act that leaves the screen identical to one nobody
// performed is the outcome ARM_NOTE exists to refuse. Each names the halves it
// did not do, because the combined action has two and the proxy one has one.
// While a write runs, its task's state line says "setting up" and this note
// says the one thing left to the user, so neither repeats the other (F19).
const APPROVE = "approve the prompt that just opened";

const setupNote = (os: HostOs): ActionNotes => ({
  refused: os === "windows" ? APPROVAL_CANCELLED : PROMPT_CANCELLED,
  running: { tone: "pending", text: APPROVE },
  nothing: {
    tone: "mute",
    text: "nothing changed — the other proxy is still the one in use, and the certificate was not installed.",
  },
  failed: { tone: "warn", text: "setting up failed — do it by hand below." },
});

// Only macOS asks before the proxy changes: Windows and GNOME write the user's
// own settings, so their notes name no prompt (W12's W2).
const setProxyNote = (os: HostOs): ActionNotes => ({
  running: {
    tone: "pending",
    text: os === "macos" ? APPROVE : "setting the proxy…",
  },
  nothing: {
    tone: "mute",
    text: "nothing changed — the other proxy is still the one in use.",
  },
  failed: {
    tone: "warn",
    text: "setting the proxy failed — do it by hand below.",
  },
});

// simctl asks for nothing, so its note has no prompt to name (W16's Q3).
const TRUST_SIMULATORS_NOTE: ActionNotes = {
  running: { tone: "pending", text: "trusting it in the Simulator…" },
  failed: {
    tone: "warn",
    text: "couldn't trust it in the Simulator — do it by hand below.",
  },
};

// Each Simulator keeps its own store (W7's N2).
const TRUST_SIMULATORS_CAPTION =
  "It trusts Trawler's certificate in every Simulator running now. Each keeps its own store: one you create or erase later needs this again.";

const OPEN_EMULATOR_NOTE: ActionNotes = {
  running: { tone: "pending", text: "opening it…" },
  failed: {
    tone: "warn",
    text: "couldn't open it — open it from Android Studio's Device Manager.",
  },
};

const OPEN_EMULATOR_CAPTION = "It starts the emulator you used last.";
const OPEN_EMULATOR_THEN_SET_UP =
  `${OPEN_EMULATOR_CAPTION} Once it runs, setting it up takes this place.`;

// adb and su ask for nothing, so no note names a prompt (W17's Q3).
const SET_UP_EMULATOR_NOTE: ActionNotes = {
  running: { tone: "pending", text: "setting up the emulator…" },
  failed: { tone: "warn", text: "setting up failed — do it by hand below." },
};

const INSTALL_EMULATORS_NOTE: ActionNotes = {
  running: { tone: "pending", text: "installing it in the emulator…" },
  failed: {
    tone: "warn",
    text: "couldn't install it in the emulator — do it by hand below.",
  },
};

const INSTALL_EMULATORS_CAPTION =
  "It installs Trawler's certificate in every emulator running now. Each keeps its own store: one you create or wipe later needs this again.";

const ROUTE_EMULATORS_NOTE: ActionNotes = {
  running: { tone: "pending", text: "routing the emulator…" },
  failed: {
    tone: "warn",
    text: "couldn't route the emulator — do it by hand below.",
  },
};

const UNROUTE_EMULATORS_NOTE: ActionNotes = {
  running: { tone: "pending", text: "turning the emulator's proxy off…" },
  failed: {
    tone: "warn",
    text: "couldn't turn the emulator's proxy off — do it by hand below.",
  },
};

const REMOVE_CA_NOTE: ActionNotes = {
  running: { tone: "pending", text: "removing the certificate…" },
  failed: {
    tone: "warn",
    text: "couldn't remove the certificate — do it by hand below.",
  },
};

const unsetProxyNote = (os: HostOs): ActionNotes => ({
  running: {
    tone: "pending",
    text:
      os === "macos"
        ? "turning the proxy off — approve the prompt…"
        : "turning the proxy off…",
  },
  failed: {
    tone: "warn",
    text: "couldn't turn the proxy off — do it by hand below.",
  },
});

// Arming is the shell's dialog and then the bind. A yes needs no note — what is
// listening is the task's state, and it says so a line above. A no does: the
// dialog closes, the button comes back, and without this the screen says
// exactly what it says after a success. Where Trawler answers is the state
// line's to say, so the note says only what happened (W14's E3).
const ARM_NOTE: ActionNotes = {
  running: { tone: "pending", text: "waiting for your answer…" },
  nothing: { tone: "mute", text: "not allowed" },
  failed: { tone: "warn", text: "couldn't start listening on that address." },
};

const DISARM_NOTE: ActionNotes = {
  running: { tone: "pending", text: "stopping…" },
  failed: { tone: "warn", text: "couldn't stop listening." },
};

// The request's note says which way it went, as the core saw it (D9) - never
// whether setup works: that is each task's own line (DEC-56 rule 1). A page
// that got its answer around Trawler is the one outcome that looked like
// success and was not.
const REQUEST_ROUTE: Record<RequestRoute, ActionNote> = {
  through: { tone: "ok", text: `${REQUEST_HOST} answered through Trawler.` },
  undecrypted: {
    tone: "warn",
    text: "the request reached Trawler and didn't decrypt.",
  },
  around: {
    tone: "warn",
    text: `${REQUEST_HOST} answered, but not through Trawler.`,
  },
};

const MAKE_REQUEST_NOTE: ActionNotes = {
  running: { tone: "pending", text: `waiting for ${REQUEST_HOST}…` },
  done: (route) => REQUEST_ROUTE[route as RequestRoute],
  failed: {
    tone: "warn",
    text: `the request to ${REQUEST_HOST} didn't complete.`,
  },
};

// The one act's prompt is the host's: macOS asks for a password, Windows for
// an approval (W12's W2), and either does the two tasks below it.
const ONE_ACT_CAPTION: Record<"macos" | "windows", string> = {
  macos: "one password prompt — it does tasks 1 and 2",
  windows: "one Windows approval — it does tasks 1 and 2",
};

const EMULATOR_ONE_ACT_CAPTION =
  "no prompt — it does tasks 1 and 2 in every emulator running";

const noAddress = (here: string, device: string) =>
  `${here} holds no network address a second device could reach it at. Join the network ${device} is on, then check again.`;

// The save resolves a full path natively and a bare name in the browser; the
// steps below name a file, so the note does too.
const fileName = (path: string) => path.split(/[/\\]/).pop() ?? path;

const saveCaNote = (named: string): ActionNotes => ({
  running: { tone: "pending", text: "saving…" },
  done: (path: string) => ({
    tone: "ok",
    text:
      fileName(path) === named
        ? `saved as ${named}. Now trust it with the steps here.`
        : `saved as ${fileName(path)} — the steps here name ${named}, so use ${fileName(path)} instead.`,
  }),
  nothing: {
    tone: "mute",
    text: "nothing was saved — the save was cancelled.",
  },
  failed: {
    tone: "warn",
    text: "couldn't save the certificate — check that Trawler is running.",
  },
});

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

// Evidence needs a clock only at one edge: the open tunnel becomes a refusal
// when its grace passes with nothing decrypted after it. So the screen re-reads
// once, at that edge, rather than ticking every task the way the traffic line does.
function useGraceEdge(tunnelAt: number | null): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (tunnelAt === null) return;
    const left = tunnelAt + TUNNEL_GRACE_MS - Date.now();
    if (left <= 0) {
      setNow(Date.now());
      return;
    }
    const id = setTimeout(() => setNow(Date.now()), left);
    return () => clearTimeout(id);
  }, [tunnelAt]);
  return now;
}

// The address's refusal, held past a retry of it until the retry settles.
function useSettledRefusal(newest: AddressRefusal | undefined): {
  refusal: AddressRefusal | undefined;
  now: number;
} {
  const now = useGraceEdge(newest?.at ?? null);
  const [held, setHeld] = useState<AddressRefusal | undefined>(undefined);
  const refusal = settledRefusal(newest, held, now);
  useEffect(() => setHeld(refusal), [refusal]);
  return { refusal, now };
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
  const failed = action.state === "failed" ? action.failure : undefined;
  const prompt = failed?.refused
    ? (note.refused ?? PROMPT_CANCELLED)
    : failed?.unshowable
      ? PROMPT_UNSHOWABLE
      : null;
  const entry = note[action.state];
  const shown = typeof entry === "function" ? entry(action.named) : entry;
  if (!shown && prompt === null) return null;
  return (
    <div
      data-testid={`${testId}-status`}
      data-state={action.state}
      className="pt-chrome-y"
    >
      <Notice tone={prompt === null ? shown!.tone : "mute"}>
        {prompt ?? shown!.text}
      </Notice>
      {action.state === "failed" && action.failure !== undefined && (
        <div className="pt-2">
          <Disclosure summary="details">
            <p
              data-wire="command output"
              className="font-mono text-chrome text-ink-mute"
            >
              {action.failure.detail}
            </p>
          </Disclosure>
        </div>
      )}
    </div>
  );
}

// Fork D4: disabled in flight AND a progress line beside it — both halves or
// neither, so a dead control is never the only signal. The caption says what
// the button does, so it waits behind the button's ? (W13's R1).
function Action({
  testId,
  label,
  caption,
  action,
  note,
  onClick,
  frame,
  disabled = false,
}: {
  testId: string;
  label: string;
  caption?: string;
  action: ActionHandle;
  note: ActionNotes;
  onClick: () => void;
  frame?: "bare";
  disabled?: boolean;
}) {
  const button = (
    <Button
      onClick={onClick}
      frame={frame}
      tone={frame === "bare" ? "mute" : undefined}
      disabled={disabled || action.state === "running"}
    >
      {label}
    </Button>
  );
  return (
    <div>
      {caption === undefined ? (
        button
      ) : (
        <Help topic="what this does" text={caption}>
          {button}
        </Help>
      )}
      <ActionNote testId={testId} action={action} note={note} />
    </div>
  );
}

const ZONE_LABEL = "font-ui text-chrome uppercase tracking-label text-ink-mute";

// The save control's outcome is this screen's note, so the shared step
// renderer is handed it rather than knowing about actions' wording.
function GuideSteps({
  steps,
  ...controls
}: { steps: GuideStep[] } & StepControls) {
  const { save } = controls;
  return (
    <SharedGuideSteps
      steps={steps}
      {...controls}
      saveNote={
        save &&
        ((format) => (
          <ActionNote
            testId="save-ca"
            action={save}
            note={saveCaNote(CA_FILES[format])}
          />
        ))
      }
    />
  );
}

// What a guide says it is before it is opened. Where a button does the job,
// the guide is the other way of doing it and says so. Where there is no
// button it is the only way, and a count is the one summary that promises no
// automatic alternative while still saying how much work is behind it (F10).
const MANUAL = "do it by hand";
const stepCount = (guide: Guide) =>
  `${guide.steps.length} step${guide.steps.length === 1 ? "" : "s"}`;

// One task's instructions: the route, then the three things a numbered step
// must not be. Notes are facts about this platform and sit under the steps
// unnumbered; a second route and the way back are each behind a summary of
// their own, so the primary path is what the reader meets.
function GuideView({
  guide,
  save,
  open,
  framed,
  summary,
  defaultOpen = false,
}: {
  guide: Guide;
  summary: string;
  defaultOpen?: boolean;
} & StepControls) {
  const alternatives = guide.alternatives ?? [];
  const undo = guide.undo ?? [];
  return (
    <Disclosure summary={summary} defaultOpen={defaultOpen}>
      <GuideSteps
        steps={withNotes(guide.steps, guide.notes)}
        save={save}
        open={open}
        framed={framed}
      />
      {alternatives.length > 0 && (
        <div className="pt-3">
          <Disclosure summary="other ways">
            <GuideSteps steps={alternatives} save={save} open={open} />
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

// A note is true of the steps and nothing to do, so it waits behind a ? at the
// end of the last one (W13's R1).
function withNotes(steps: GuideStep[], notes?: string[]): GuideStep[] {
  if (!notes?.length || steps.length === 0) return steps;
  return [
    ...steps.slice(0, -1),
    [...steps[steps.length - 1], { help: notes.join(" "), topic: "a note on these steps" }],
  ];
}

// This machine's other ways to make the request: its browser, where a route
// opens one, then a terminal.
function tryOtherWays(setup: PlatformGuides): GuideStep[] {
  const routes = (setup.tryRoutes ?? []).map((route): GuideStep => [
    "Open it in your browser",
    route,
  ]);
  const command: GuideStep[] = setup.tryCommand
    ? [
        [
          setup.tryLead ?? "In a terminal, run:",
          { command: setup.tryCommand, label: "the try-it command" },
        ],
      ]
    : [];
  return [...routes, ...command];
}

// The request leads the try task while it is the next thing to do, and once
// something decrypted it folds, with its other ways, under one summary: the
// proof is the task's line, and another request is optional. A request the
// user just made keeps its outcome in view (W12's E2, E3).
function TryRequest({
  request,
  framed,
  folded,
  other,
  open,
}: {
  request: ActionHandle;
  framed: boolean;
  folded: boolean;
  other: GuideStep[];
  open?: (route: RouteName) => void;
}) {
  const body = (
    <>
      {request.trigger && (
        <Action
          testId="make-request"
          label={`make a request to ${REQUEST_HOST}`}
          action={request}
          note={MAKE_REQUEST_NOTE}
          onClick={request.trigger}
          frame={framed ? undefined : "bare"}
        />
      )}
      {other.length > 0 && (
        <div
          data-testid="try-other-ways"
          className={request.trigger ? "pt-3" : ""}
        >
          <Disclosure summary="other ways">
            <GuideSteps steps={other} open={open} />
          </Disclosure>
        </div>
      )}
    </>
  );
  return (
    <div data-testid="try-request" className="pt-2">
      {folded ? (
        <Disclosure
          summary="make another request"
          defaultOpen={request.state !== "idle"}
        >
          {body}
        </Disclosure>
      ) : (
        body
      )}
    </div>
  );
}

// The outcome reads where the request that produces it is made, which is the one
// thing DEC-13 accepts as proof that setup is over.
function ProvenOutcome({
  device,
  traffic,
}: {
  device: string | null;
  traffic: Traffic;
}) {
  const now = useNow();
  if (trafficReading(traffic, now) !== "capturing") return null;
  return (
    <div data-testid="proven" className="px-4">
      <Notice
        tone="ok"
        detail={
          <span>
            last request{" "}
            <TimestampText
              timestamp={traffic.lastDecryptedAt}
              format="relative"
            />
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
function TrafficState({ read }: { read: (now: number) => StepStatusView }) {
  const now = useNow();
  const status = read(now);
  return (
    <>
      <div data-testid="traffic-state">
        <StatusLine status={status} />
      </div>
      {status.remedy !== undefined && (
        <div data-testid="traffic-remedy" className="pt-2">
          <Notice tone="mute">{status.remedy}</Notice>
        </div>
      )}
    </>
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
  const chosen = addresses.find((address) => address.address === value);
  if (chosen === undefined) return null;
  const others = addresses.filter((address) => address.address !== value);
  return (
    <>
      <p className={ZONE_LABEL}>address to use</p>
      <p
        data-testid="listen-on"
        className="pt-2 font-ui text-body text-ink-body"
      >
        {addressLabel(chosen)}
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
  device,
  here,
  reach,
  lost,
  chosen,
  onChoose,
  onReread,
  arm,
  disarm,
}: {
  n: number;
  title: string;
  device: string;
  here: string;
  reach: ReachState;
  lost: string | null;
  chosen: string | undefined;
  onChoose: (address: string) => void;
  onReread?: () => void;
  arm: ActionHandle;
  disarm: ActionHandle;
}) {
  if (reach.armed && reach.proxy !== null && lost === null) {
    const { host } = splitProxyAddr(reach.proxy);
    return (
      <TaskStep
        n={n}
        title={title}
        status={verdict("done", `listening on ${host} until Trawler quits`)}
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

  // An address this machine left is still listened on and reaches nothing, so
  // the task is offered again on the address it holds now (R1).
  const network = reach.interfaces.find((i) => i.address === chosen);
  return (
    <TaskStep
      n={n}
      title={title}
      status={
        lost === null
          ? verdict("not-done", `Trawler answers on ${here} only`)
          : verdict("not-working", `${here} no longer holds ${lost}`)
      }
    >
      {network === undefined ? (
        <>
          <Notice tone="mute">{noAddress(here, device)}</Notice>
          {onReread && (
            <div data-testid="reread-reach" className="pt-2">
              <Button onClick={onReread} frame="bare" tone="mute">
                check again
              </Button>
            </div>
          )}
        </>
      ) : (
        <>
          <AddressChoice
            addresses={reach.interfaces}
            value={network.address}
            onChange={onChoose}
          />
          {arm.trigger && (
            <div className="pt-3">
              <Action
                testId="arm-reach"
                label={`allow devices on ${network.name} to connect`}
                caption={`other devices on ${network.name} will be able to use this proxy and see captured traffic until Trawler quits`}
                action={arm}
                note={ARM_NOTE}
                onClick={arm.trigger}
              />
            </div>
          )}
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

// A task's guide as its reading leaves it. A task read done keeps only its way
// back; one not done keeps its route and nothing of undoing it, since nothing is
// there to undo yet (W12's E4). Under a button, the steps are that button's by
// hand, whichever way it goes (W12's W1).
interface TaskGuide {
  guide: Guide;
  summary: string;
}

function guideFor(
  guide: Guide | undefined,
  done: boolean,
  underButton: boolean,
): TaskGuide | undefined {
  if (guide === undefined) return undefined;
  if (!done)
    return {
      guide: { ...guide, undo: undefined },
      summary: underButton ? MANUAL : stepCount(guide),
    };
  if (guide.unprobed)
    return {
      guide: { ...guide, steps: guide.unprobed },
      summary: underButton ? MANUAL : stepCount({ steps: guide.unprobed }),
    };
  return guide.undo?.length
    ? {
        guide: { steps: guide.undo },
        summary: underButton ? MANUAL : (guide.undoTitle ?? "to undo it later"),
      }
    : undefined;
}

// A task's instructions, and the state of the task they belong to. `dim` is the
// one thing a task with no instructions needs: it keeps its number and says
// which step comes first, rather than explaining what cannot happen yet (F9).
// Where the first step cannot be done either, it says nothing it waits on (R22).
// The guide is keyed on its summary and its open answer, so it follows the
// rule again whenever the rule's answer changes — a failure that opens it, a
// reading that folds it to its way back — and is the user's to toggle between.
function GuidedTask({
  n,
  task,
  status,
  dim,
  waitsOn,
  guide,
  guideOpen = false,
  save,
  open,
  framed,
  action,
  children,
}: {
  n: number;
  task: Task;
  status?: StepStatusView;
  dim?: boolean;
  waitsOn?: string;
  guide?: TaskGuide;
  guideOpen?: boolean;
  action?: ReactNode;
  children?: ReactNode;
} & StepControls) {
  const body =
    guide !== undefined || action !== undefined || children !== undefined;
  return (
    <TaskStep
      n={n}
      title={task.title}
      help={dim ? undefined : task.why}
      waitsOn={dim ? waitsOn : undefined}
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
                key={`${guide.summary}:${guideOpen}`}
                guide={guide.guide}
                save={save}
                open={open}
                framed={framed}
                summary={guide.summary}
                defaultOpen={guideOpen}
              />
            </div>
          )}
        </>
      )}
    </TaskStep>
  );
}

// A failure the user did not choose, from an action the screen still offers.
// A dismissed prompt is the user's own answer and the button can still do the
// job, so it opens nothing (Q8).
const failedHere = (action: ActionHandle, offered: boolean) =>
  offered && action.state === "failed" && action.failure?.refused !== true;

interface SetupProps {
  os?: HostOs;
  proxyAddr?: string;
  trustProbe?: TrustProbe;
  proxy?: ProxyReading;
  probing?: boolean;
  traffic?: Traffic;
  ownProofAt?: number | null;
  simulators?: readonly SimulatorTrust[] | null;
  // `null` is no reading: adb not found, or a read not landed (W17's W4).
  emulators?: readonly EmulatorReading[] | null;
  androidSdk?: AndroidSdk | null;
  // adb lists an emulator that has not finished booting (W17's S2).
  emulatorStarting?: boolean;
  savedCa?: SavedCa;
  // Another device (DEC-63) - the Simulator, whose wire is this machine's too,
  // and the emulator, which arrives on its loopback, among them: the mark its
  // address carries, and what its tasks read beside the traffic that is its.
  deviceMark?: string;
  device?: AnotherDevice;
  reach?: ReachState;
  onSetUpThisMachine?: () => Promise<null | void>;
  onSetUpSimulator?: () => Promise<null | void>;
  onTrustSimulators?: () => Promise<void>;
  onSetUpEmulator?: () => Promise<null | void>;
  onInstallOnEmulators?: () => Promise<void>;
  onRouteEmulators?: () => Promise<void>;
  onUnrouteEmulators?: () => Promise<void>;
  onSaveCa?: (format: CaFormat) => Promise<string | null>;
  onSetProxy?: () => Promise<null | void>;
  onUnsetProxy?: () => Promise<void>;
  onRemoveCa?: () => Promise<void>;
  onArmReach?: (address: string) => Promise<null | void>;
  onDisarmReach?: () => Promise<void>;
  onRereadReach?: () => void;
  onMakeRequest?: () => Promise<RequestRoute>;
  onOpenRoute?: (route: RouteName) => Promise<void>;
  routeAct?: RouteAct;
  onRouteAct?: (act: RouteAct) => void;
}

export type RouteAct = "set" | "unset" | null;

// Which act last changed this machine's proxy, until a probe answers for it.
// DEC-13's inherited rule is that work which changed this host stays
// reversible from the moment it ran, so an act that set the proxy opens the
// way out before the re-read lands — and one that took it off closes it, since
// until the re-read lands the old reading still says ours. A conclusive
// reading replaces both; one that could not read replaces neither, because a
// probe that never answers may not strand a machine this app just changed.
export function useRouteMemory(
  status: ProxyReading["status"],
): [RouteAct, (act: RouteAct) => void] {
  const [act, setAct] = useState<RouteAct>(null);
  useEffect(() => {
    if (status !== "unknown") setAct(null);
  }, [status]);
  return [act, setAct];
}

// The screen asks which device before it instructs one. Asked to set up another
// device, it leaves the question open rather than answer the device already
// set up (R6); every task below belongs to the answer, so none is drawn without
// one.
export function Onboarding({
  state,
  dispatch,
  ...setup
}: SetupProps & { state: OnbState; dispatch: (e: OnbEvent) => void }) {
  const [platform, setPlatform] = useState(() => opensOn(state));
  const choose = (id: PlatformId) => {
    setPlatform(id);
    dispatch({ t: "choose", platform: id });
  };

  return (
    <div className="flex h-full flex-col overflow-auto">
      <div className="mx-auto flex w-full max-w-reading flex-col gap-6 py-8">
        <div data-testid="lead" className="px-4">
          <p className="font-serif text-note text-ink">{TITLE}</p>
          <p className="pt-2 font-ui text-body text-ink-body">{PURPOSE}</p>
        </div>

        <div
          data-testid="platform"
          className="flex flex-wrap items-center gap-3 px-4"
        >
          <p className={ZONE_LABEL}>{QUESTION}</p>
          <Select
            options={platformChoices(setup.os ?? "other")}
            value={platform ?? undefined}
            onChange={(id) => choose(id as PlatformId)}
            label={QUESTION}
            placeholder={UNANSWERED}
            searchable={false}
            className="w-picker"
          />
        </div>

        {platform === null ? (
          <ProvenOutcome device={null} traffic={setup.traffic ?? NO_TRAFFIC} />
        ) : (
          <PlatformSetup platform={platform} {...setup} />
        )}

        <BackToFlows onBack={() => dispatch({ t: "exit" })} />
      </div>
    </div>
  );
}

// The way out is on every setup screen, proven or not (DEC-61): the list is a
// screen to switch to, not a reward for finishing.
export function BackToFlows({ onBack }: { onBack: () => void }) {
  return (
    <div data-testid="back" className="px-4">
      <Button onClick={onBack} frame="bare" tone="mute">
        <span aria-hidden="true">‹ </span>
        {COPY.back}
      </Button>
    </div>
  );
}

// Another device's tasks read the traffic they are handed unless told this
// machine's own may be in it.
const UNMIXED: AnotherDevice = {
  provedAt: null,
  mixed: false,
  wire: NO_TRAFFIC,
};
const NO_SIMULATORS: readonly SimulatorTrust[] = [];
const NO_EMULATORS: readonly EmulatorReading[] = [];

function PlatformSetup({
  platform,
  os = "other",
  proxyAddr = "127.0.0.1:7420",
  trustProbe = "unknown",
  proxy = NO_PROXY_READING,
  probing = false,
  traffic = NO_TRAFFIC,
  ownProofAt = null,
  simulators = NO_SIMULATORS,
  emulators = NO_EMULATORS,
  androidSdk = null,
  emulatorStarting = false,
  savedCa,
  deviceMark,
  device = UNMIXED,
  reach = UNREACHABLE,
  onSetUpThisMachine,
  onSetUpSimulator,
  onTrustSimulators,
  onSetUpEmulator,
  onInstallOnEmulators,
  onRouteEmulators,
  onUnrouteEmulators,
  onSaveCa,
  onSetProxy,
  onUnsetProxy,
  onRemoveCa,
  onArmReach,
  onDisarmReach,
  onRereadReach,
  onMakeRequest,
  onOpenRoute,
  routeAct = null,
  onRouteAct,
}: SetupProps & { platform: PlatformId }) {
  const [listenOn, setListenOn] = useState<string | null>(null);
  const open =
    onOpenRoute &&
    ((route: RouteName) => void onOpenRoute(route).catch(() => undefined));

  const routing =
    (act: "set" | "unset") =>
    <T,>(outcome: T): T => {
      if (outcome !== null) onRouteAct?.(act);
      return outcome;
    };

  const ownStore = probesOwnStore(platform);
  const adb = readsOverAdb(platform);
  // The emulator's one act changes no proxy of this machine's, so it leaves
  // the memory of this machine's route alone.
  const onSetUp = ownStore
    ? onSetUpSimulator
    : adb
      ? onSetUpEmulator
      : onSetUpThisMachine;
  const setUp = useAction(
    onSetUp && (() => onSetUp().then(adb ? (outcome) => outcome : routing("set"))),
  );
  const trustSimulators = useAction(onTrustSimulators);
  const stores = simulators ?? NO_SIMULATORS;
  const simulatorsRunning = stores.length > 0;
  const installEmulators = useAction(onInstallOnEmulators);
  const routeEmulators = useAction(onRouteEmulators);
  const unrouteEmulators = useAction(onUnrouteEmulators);
  const openEmulator = useAction(
    onOpenRoute && (() => onOpenRoute("emulator")),
  );
  const running = emulators ?? NO_EMULATORS;
  const emulatorsRunning = running.length > 0;
  // adb read none running. The screen keeps the layout one running will show,
  // and what acts on one waits for it (W17's S1).
  const emulatorDown = adb && emulators !== null && !emulatorsRunning;
  // su answered on every one running, so the shell writes each store (W17's
  // I1); with none, the AVD used last says whether it will (W17's S3).
  const emulatorsRooted = emulatorsRunning
    ? running.every((e) => e.trust !== "unknown")
    : emulatorDown && androidSdk?.avd?.playStore !== true;
  const saveCa = useAction(onSaveCa && (() => onSaveCa(caFormatFor(platform))));
  const setProxyAction = useAction(
    onSetProxy && (() => onSetProxy().then(routing("set"))),
  );
  const unsetProxyAction = useAction(
    onUnsetProxy && (() => onUnsetProxy().then(routing("unset"))),
  );
  const removeCa = useAction(onRemoveCa);
  const chosen =
    reach.interfaces.find((i) => i.address === listenOn)?.address ??
    reach.interfaces[0]?.address;
  const arm = useAction(
    onArmReach && chosen !== undefined ? () => onArmReach(chosen) : undefined,
  );
  const disarm = useAction(onDisarmReach);
  // A request this app makes travels through this host, so it proves this
  // host's setup and no other device's (DEC-56).
  const request = useAction(platform === "this" ? onMakeRequest : undefined);

  const lost = lostAddress(reach);
  const allowed =
    reach.armed &&
    reach.proxy !== null &&
    reach.ca_urls !== null &&
    lost === null;
  const setup = setupFor(platform, {
    os,
    proxyAddr,
    reach: allowed ? { proxy: reach.proxy!, caUrls: reach.ca_urls!, wifiPage: reach.wifi_page } : null,
    saved: savedCa,
    simulatorsRunning: simulators === null ? undefined : simulatorsRunning,
    emulator: {
      running: emulators === null ? undefined : emulatorsRunning,
      rooted: emulators === null ? undefined : emulatorsRooted,
      sdk: androidSdk ?? undefined,
    },
  });
  const here = deviceName("this", os);

  const autoHost = canAutoSetup(os);
  // An action that changes this machine is offered for the store it changes,
  // and only where the chosen platform reads that store: installing here does
  // nothing for a Simulator with a keychain of its own, and setting this
  // machine's proxy is exactly what the Simulator routes through.
  const trustHere = probesTrust(platform) && autoHost;
  const routeHere = probesRoute(platform) && autoHost;
  // Each task has one reading. A probe that concludes is it (DEC-33). Where no
  // probe concludes on This machine, its own request is (DEC-62): the try-it
  // command names the proxy itself, and a request from any other device says
  // nothing about this one (D7). Elsewhere - the Simulator's route included,
  // when this Mac's proxy cannot be read - this setup's traffic is, because
  // that is what the task promised to be confirmed by; another device's is
  // only what could have come from it (DEC-63). The Simulator's wire is this
  // machine's too, so its certificate reads its own store, else its marked
  // request, and nothing unmarked is its own (W7). The emulator's store and
  // proxy are adb's to read; where adb reads neither, it is another device that
  // needs no allow (W8).
  // Done is the reading, never what the state line says this second: a
  // re-check in flight changes the line and may not change the layout.
  const offMachine = needsReach(platform);
  const sharedWire = sharesHostWire(platform);
  const anotherDevice = offMachine || adb;
  const mixed = (anotherDevice && device.mixed) || sharedWire;
  const graceEdge = useGraceEdge(traffic.openTunnelAt);
  const evidence = evidenceOf(traffic, graceEdge);
  const routeEvidence = sharedWire
    ? evidenceOf(device.wire, graceEdge)
    : evidence;
  const ownProved = platform === "this" && ownProofAt !== null;
  const first = offMachine ? 1 : 0;
  const tryStep = first + 3;
  const reading: DeviceReading = {
    device: deviceName(platform, os),
    provedAt: device.provedAt,
    mixed,
    tryStep,
  };
  // A phone's own services refuse whatever its certificate does, so only its
  // address refused is that certificate's to say, and it outlasts a proof (W9, W10).
  const addressTest = setup.addressTest;
  const address = useSettledRefusal(device.refusal);
  const addressRefused =
    addressTest === undefined
      ? null
      : refusedAddress(address.refusal, evidence, reading, address.now);
  const trustUnread = probesTrust(platform) && trustProbe === "unknown";
  const trustDone = probesOwnStore(platform)
    ? (storesTrusted(stores) ?? device.provedAt !== null)
    : adb
      ? (storesTrusted(running) ?? evidence.decrypted)
      : !probesTrust(platform)
        ? evidence.decrypted && addressRefused === null
        : trustUnread
          ? ownProved
          : trustProbe === "trusted";
  const routeUnread = probesRoute(platform) && proxy.status === "unknown";
  const routeByOwnRequest = platform === "this";
  const routeDone =
    probesRoute(platform) && !routeUnread
      ? proxy.status === "ours"
      : routeByOwnRequest
        ? ownProved
        : adb
          ? (emulatorsRouted(running) ?? evidence.reached)
          : routeEvidence.reached;
  // One prompt for the one thing most users want, and only while both halves
  // are this machine's to change and neither reads done (F27). Once either
  // does, each task carries its own, and the certificate never gets an
  // install-only button (UC-ONBOARD). The Simulator's store is the shell's to
  // write wherever one runs, with no prompt of its own (W16's Q2), and so are
  // the emulator's store and proxy wherever su answers (W17's Q2).
  const oneActStore = ownStore
    ? simulatorsRunning
    : adb
      ? emulatorsRooted
      : trustHere;
  const oneActRoute = adb
    ? routeEmulators.trigger !== undefined
    : routeHere && setProxyAction.trigger !== undefined;
  // The one act's place is above the tasks whether or not it can run yet: with
  // no emulator running, the button that starts one stands there instead.
  const setUpLeads =
    oneActStore &&
    oneActRoute &&
    !trustDone &&
    !routeDone &&
    setUp.trigger !== undefined;
  const setUpHere = setUpLeads && !emulatorDown ? setUp.trigger : undefined;
  const installEmulatorsHere =
    adb && emulatorsRooted && !setUpLeads && !trustDone
      ? installEmulators.trigger
      : undefined;
  const routeEmulatorsHere =
    adb && (emulatorsRunning || emulatorDown) && !setUpLeads && !routeDone
      ? routeEmulators.trigger
      : undefined;
  // Its reversal answers the proxy adb read, never traffic that only reached.
  const unrouteEmulatorsHere =
    adb && emulatorsRouted(running) === true
      ? unrouteEmulators.trigger
      : undefined;
  const trustSimulatorsHere =
    ownStore && simulatorsRunning && !setUpLeads && !trustDone
      ? trustSimulators.trigger
      : undefined;
  const setProxyHere =
    routeHere && !setUpLeads && !routeDone
      ? setProxyAction.trigger
      : undefined;
  const reverseHere =
    routeHere && unsetProxyAction.trigger && routeAct !== "unset"
      ? routeDone || routeAct === "set"
      : false;
  const removeHere =
    trustHere && os === "macos" && trustDone ? removeCa.trigger : undefined;

  // The action the user just took outranks the re-read it triggers, and the one
  // combined action outranks both halves it is running.
  const busy = (own: ActionHandle, r: Running): Running =>
    setUp.state === "running" || own.state === "running"
      ? r
      : probing
        ? "checking"
        : null;

  // Off-machine, every task after the network one needs an address that does
  // not exist yet — so they keep their numbers and say which step comes first.
  const blocked = offMachine && !allowed;
  const waitsOn = chosen === undefined ? undefined : "after task 1";

  const trustState = ownStore
    ? simulatorTrustStatus(
        simulators,
        busy(trustSimulators, "installing"),
        reading,
      )
    : probesTrust(platform)
      ? trustStatus(
          trustProbe,
          busy(setUp, "installing"),
          byOwnRequest("trust", ownProofAt, tryStep),
          setup.trust.guide?.unprobed && SYSTEM_TOOLS_TRUSTED,
        )
      : adb
        ? emulatorTrustStatus(
            emulators,
            busy(installEmulators, "installing"),
            reading,
            evidence,
            confirmHttpsOn(platform, os),
            emulatorStarting,
          )
        : addressTest
          ? addressTrustStatus(
              evidence,
              reading,
              addressRefused,
              confirmHttpsOn(platform, os),
            )
          : deviceTrustStatus(evidence, reading, confirmHttpsOn(platform, os));
  const routeState = probesRoute(platform)
    ? proxyStatus(proxy.status, busy(setProxyAction, "setting"), {
        device: here,
        unread: routeByOwnRequest
          ? byOwnRequest("route", ownProofAt, tryStep)
          : routeByTraffic(routeEvidence, setup.confirmOn),
      })
    : adb
      ? emulatorRouteStatus(
          running,
          busy(routeEmulators, "setting"),
          reading,
          evidence,
          setup.confirmOn,
        )
      : deviceRouteStatus(evidence, reading, setup.confirmOn);
  // A tunnel nothing decrypted says the certificate is the step left, so the
  // certificate task says it, once, while it is not done; once it is, the
  // refusal is some other client's and the try task words it instead. So does
  // the try task wherever the refusal may be any device's: This machine's
  // store unread (D7), another device's traffic mixed with this one's (A3), and
  // the emulator, whose own services refuse a certificate it holds (W8's S2).
  const emulatorRefused = adb
    ? emulatorRefusal(trustedEmulators(running))
    : undefined;
  const certificateOwnsRefusal = addressTest
    ? addressRefused !== null
    : !trustDone && !trustUnread && !mixed && !adb;
  const refusedHere =
    certificateOwnsRefusal && (addressRefused !== null || evidence.unanswered);
  const refusalNote =
    addressTest !== undefined && addressRefused !== null ? (
      addressTest.refused
    ) : setup.notDecrypting === undefined ? (
      notDecryptingHere(deviceName(platform, os))
    ) : (
      <Help topic="if you already did" text={setup.notDecrypting}>
        {notDecryptingHere(deviceName(platform, os))}
      </Help>
    );

  const trustGuide = guideFor(
    setup.trust.guide,
    trustDone,
    trustDone
      ? removeHere !== undefined
      : setUpLeads ||
          trustSimulatorsHere !== undefined ||
          installEmulatorsHere !== undefined,
  );
  const routeUnderButton =
    setUpLeads ||
    setProxyHere !== undefined ||
    routeEmulatorsHere !== undefined;
  const routeGuide = guideFor(
    setup.route.guide,
    routeDone,
    routeDone
      ? reverseHere || unrouteEmulatorsHere !== undefined
      : routeUnderButton,
  );

  // One guide is open. A failed action's route comes first, because its note
  // says the steps below do it by hand. Otherwise it is the first whose task is
  // not done: everything above it is finished and everything below it waits on
  // this one. A guide under a button is shut while the button can still do the
  // job — the button is the route, and the steps are what a reader asks for.
  // Nothing observes a task on another device, so off-machine the first guided
  // task stays open until a decrypted request says the whole job is over.
  const failedRoute =
    failedHere(setUp, setUpHere !== undefined) ||
    failedHere(trustSimulators, trustSimulatorsHere !== undefined) ||
    failedHere(installEmulators, installEmulatorsHere !== undefined) ||
    failedHere(removeCa, removeHere !== undefined)
      ? "trust"
      : failedHere(setProxyAction, setProxyHere !== undefined) ||
          failedHere(routeEmulators, routeEmulatorsHere !== undefined) ||
          failedHere(unrouteEmulators, unrouteEmulatorsHere !== undefined) ||
          failedHere(unsetProxyAction, reverseHere)
        ? "route"
        : null;
  // Every sentence that says "the steps below" has them open — a failure's note
  // and the certificate task's account of a refusal — each in its own task, so
  // one never closes the steps another points at. Only where nothing points is
  // the one open guide chosen by reading order.
  const trustPointed = failedRoute === "trust" || refusedHere;
  const routePointed = failedRoute === "route";
  const byReading = !trustPointed && !routePointed;
  const guidesOpen = !blocked && traffic.lastDecryptedAt === null;
  const trustGuideOpen =
    trustPointed ||
    (byReading &&
      guidesOpen &&
      trustGuide !== undefined &&
      !trustDone &&
      !setUpLeads &&
      trustSimulatorsHere === undefined &&
      installEmulatorsHere === undefined);
  const routeGuideOpen =
    routePointed ||
    (byReading &&
      guidesOpen &&
      !trustGuideOpen &&
      routeGuide !== undefined &&
      !routeDone &&
      !routeUnderButton);

  // One control on the screen is framed, and it is the next thing to do: the
  // one act, else the first task not done's own control - its action, else its
  // open guide's first - else the request once both halves read done.
  // Everything else is bare (W12's E1, E2; W16's E2).
  const proved = traffic.lastDecryptedAt !== null;
  const requestHere = platform === "this" || setup.deviceRoute !== undefined;
  // A phone's request code proves nothing until its traffic reaches Trawler, so
  // it waits on the route, and then it is the next thing. While this machine's
  // traffic may be arriving too, the address is what the tasks above name as
  // their test, so it stays, and leads once anything arrives (W18's E2).
  const mayBePhones =
    routeDone || (mixed && evidenceOf(device.wire, graceEdge).reached);
  const requestWaits = scansCodes(platform) && !routeDone && !mixed;
  const requestCodeLeads = scansCodes(platform) && mayBePhones && !proved;
  type Next =
    | "set-up"
    | "trust-simulators"
    | "install-emulators"
    | "trust-guide"
    | "set-proxy"
    | "route-emulators"
    | "route-guide"
    | "request"
    | "start"
    | null;
  const byOrder: Next = requestCodeLeads
    ? "request"
    : setUpHere
      ? "set-up"
      : trustSimulatorsHere
        ? "trust-simulators"
        : installEmulatorsHere
          ? "install-emulators"
          : trustGuideOpen && !trustDone
            ? "trust-guide"
            : setProxyHere
              ? "set-proxy"
              : routeEmulatorsHere
                ? "route-emulators"
                : routeGuideOpen && !routeDone
                  ? "route-guide"
                  : requestHere && trustDone && routeDone && !proved
                    ? "request"
                    : null;
  // With none running, everything left to do waits on one: what starts it is
  // the next thing, wherever it can be clicked (W17's S1).
  const start = emulatorDown ? setup.start : undefined;
  const startHere =
    start !== undefined &&
    "open" in start &&
    !emulatorStarting &&
    openEmulator.trigger !== undefined;
  const next: Next = !emulatorDown
    ? byOrder
    : startHere && !(trustDone && routeDone && proved)
      ? "start"
      : null;
  const bare = (framed: boolean) => (framed ? undefined : ("bare" as const));

  // While none runs, the place the one act will stand starts the emulator; a
  // start already under way leaves the button waiting (W17's S1, S2).
  const opener =
    start === undefined ? null : !("open" in start) ? (
      <div
        data-testid="one-act"
        className="px-4 font-ui text-body text-ink-body"
      >
        <StepParts step={start} />
      </div>
    ) : (
      <div data-testid="one-act" className="px-4">
        <Action
          testId="open-emulator"
          label={start.label}
          caption={
            setUpLeads ? OPEN_EMULATOR_THEN_SET_UP : OPEN_EMULATOR_CAPTION
          }
          action={openEmulator}
          note={OPEN_EMULATOR_NOTE}
          onClick={openEmulator.trigger ?? (() => undefined)}
          frame={bare(next === "start")}
          disabled={!startHere}
        />
      </div>
    );

  // It does both tasks, so it belongs to neither: it leads them, and says which
  // it does (W12's E1).
  const oneAct = setUpHere && (
    <div data-testid="one-act" className="px-4">
      <Action
        testId="set-up"
        label={`set up ${ownStore || adb ? deviceName(platform, os) : here}`}
        caption={
          adb
            ? EMULATOR_ONE_ACT_CAPTION
            : ONE_ACT_CAPTION[os === "windows" ? "windows" : "macos"]
        }
        action={setUp}
        note={adb ? SET_UP_EMULATOR_NOTE : setupNote(os)}
        onClick={setUpHere}
        disabled={probing}
      />
    </div>
  );

  // The action and its reversal are two controls, not two halves of one, and a
  // reversal that failed leaves the way out open: the proxy it could not turn
  // off is still set. Each names the half it performs: stopping the proxy
  // leaves the certificate, and removing the certificate leaves the proxy.
  const routeAction = (setProxyHere ||
    reverseHere ||
    routeEmulatorsHere ||
    unrouteEmulatorsHere) && (
    <>
      {routeEmulatorsHere && (
        <Action
          testId="route-emulators"
          label="route the emulator"
          frame={bare(next === "route-emulators")}
          action={routeEmulators}
          note={ROUTE_EMULATORS_NOTE}
          onClick={routeEmulatorsHere}
          disabled={probing || emulatorDown}
        />
      )}
      {unrouteEmulatorsHere && (
        <Action
          testId="unroute-emulators"
          label="stop routing the emulator"
          action={unrouteEmulators}
          note={UNROUTE_EMULATORS_NOTE}
          onClick={unrouteEmulatorsHere}
          frame="bare"
          disabled={probing}
        />
      )}
      {setProxyHere && (
        <Action
          testId="set-proxy"
          label="set the system proxy"
          frame={bare(next === "set-proxy")}
          action={setProxyAction}
          note={setProxyNote(os)}
          onClick={setProxyHere}
          disabled={probing}
        />
      )}
      {reverseHere && unsetProxyAction.trigger && (
        <div className={setProxyHere ? "pt-3" : ""}>
          <Action
            testId="unset-proxy"
            label="stop routing through Trawler"
            caption={stopRoutingNote(trustDone)}
            action={unsetProxyAction}
            note={unsetProxyNote(os)}
            onClick={unsetProxyAction.trigger}
            frame="bare"
            disabled={probing}
          />
        </div>
      )}
    </>
  );

  // Keyed on the platform so a switch opens each guide as the new platform's
  // rule says, not as the user left the one in the same place before it (R11).
  return (
    <>
      <div key={platform} data-testid="steps" className="flex flex-col gap-4">
        {needsReach(platform) && (
          <NetworkTask
            n={1}
            title={reachTaskTitle(platform, os)}
            device={deviceName(platform, os)}
            here={here}
            reach={reach}
            lost={lost}
            chosen={chosen}
            onChoose={setListenOn}
            onReread={onRereadReach}
            arm={arm}
            disarm={disarm}
          />
        )}

        {opener ?? oneAct}

        <GuidedTask
          n={first + 1}
          task={setup.trust}
          status={trustState}
          dim={blocked}
          waitsOn={waitsOn}
          guide={trustGuide}
          guideOpen={trustGuideOpen}
          save={onSaveCa ? saveCa : undefined}
          open={open}
          framed={next === "trust-guide"}
          action={
            (trustSimulatorsHere && (
              <Action
                testId="trust-simulators"
                label="trust in the Simulator"
                caption={TRUST_SIMULATORS_CAPTION}
                action={trustSimulators}
                note={TRUST_SIMULATORS_NOTE}
                onClick={trustSimulatorsHere}
                frame={bare(next === "trust-simulators")}
                disabled={probing}
              />
            )) ||
            (installEmulatorsHere && (
              <Action
                testId="install-emulators"
                label="install in the emulator"
                caption={INSTALL_EMULATORS_CAPTION}
                action={installEmulators}
                note={INSTALL_EMULATORS_NOTE}
                onClick={installEmulatorsHere}
                frame={bare(next === "install-emulators")}
                disabled={probing || emulatorDown}
              />
            )) ||
            (removeHere && (
              <Action
                testId="remove-ca"
                label="remove the certificate"
                caption={removeCaNote(routeDone)}
                action={removeCa}
                note={REMOVE_CA_NOTE}
                onClick={removeHere}
                frame="bare"
                disabled={probing}
              />
            ))
          }
        >
          {refusedHere && (
            <div data-testid="traffic-remedy" className="pb-2">
              <Notice tone="mute">{refusalNote}</Notice>
            </div>
          )}
        </GuidedTask>

        <GuidedTask
          n={first + 2}
          task={setup.route}
          status={routeState}
          dim={blocked}
          waitsOn={waitsOn}
          guide={routeGuide}
          guideOpen={routeGuideOpen}
          open={open}
          framed={next === "route-guide"}
          action={routeAction || undefined}
        >
          {proxy.status === "foreign" && probesRoute(platform) && (
            <div data-testid="foreign-proxy" className="pb-2">
              <Notice tone="mute">
                {foreignProxyNotice(here, proxy.address)}
              </Notice>
            </div>
          )}
          {needsReach(platform) && allowed && (
            <ServerAndPort proxy={reach.proxy!} />
          )}
        </GuidedTask>

        <GuidedTask
          n={tryStep}
          task={setup.try}
          dim={blocked || requestWaits}
          waitsOn={blocked ? waitsOn : `after task ${first + 2}`}
        >
          <TrafficState
            read={(now) => {
              const addressNow =
                addressTest !== undefined &&
                addressRefusedNow(device.refusal, now);
              return mixed
                ? mixedTryStatus(device.wire, now, traffic.lastDecryptedAt, {
                    refusedAfterProof: addressNow
                      ? addressTest.maybeRefused
                      : setup.refusedAfterProof,
                    trustedOn:
                      probesOwnStore(platform) && storesTrusted(stores)
                        ? trustedSimulators(stores)
                        : null,
                    refused:
                      emulatorRefused ??
                      (addressNow
                        ? addressTest.maybeRefused
                        : addressTest?.elsewhereRefused),
                    ownsRefusal: certificateOwnsRefusal,
                  })
                : trafficStatus(traffic, now, {
                    ownsRefusal: certificateOwnsRefusal,
                    trustedOn: platform === "this" && trustDone ? here : null,
                    proofStays:
                      platform === "this"
                        ? "once-quiet"
                        : anotherDevice
                          ? "past-anything"
                          : "never",
                    refusedAfterProof: setup.refusedAfterProof,
                    refused: emulatorRefused ?? addressTest?.elsewhereRefused,
                  });
            }}
          />
          {(request.trigger || setup.tryCommand) && (
            <TryRequest
              request={request}
              framed={next === "request"}
              folded={proved}
              other={tryOtherWays(setup)}
              open={open}
            />
          )}
          {open && setup.deviceRoute && (
            <div data-testid="open-on-device" className="pt-2">
              <Explained help={setup.deviceRouteHelp}>
                <Button
                  onClick={() => open(setup.deviceRoute!.open)}
                  frame={bare(next === "request")}
                  tone={next === "request" ? undefined : "mute"}
                  disabled={setup.deviceRoute.disabled}
                >
                  {setup.deviceRoute.label}
                </Button>
              </Explained>
            </div>
          )}
          {deviceMark !== undefined && (
            <div
              data-testid="check-address"
              className="pt-2 font-ui text-body text-ink-body"
            >
              <StepParts
                framed={next === "request"}
                step={checkStep(
                  platform,
                  os,
                  markedUrl(deviceMark),
                  addressTest !== undefined && traffic.lastDecryptedAt !== null,
                )}
              />
            </div>
          )}
        </GuidedTask>
      </div>

      <ProvenOutcome
        device={
          platform === "this"
            ? provenDevice(here, trustProbe, proxy.status)
            : marksItsOwn(platform)
              ? markedDevice(deviceName(platform, os), device.provedAt, traffic)
              : null
        }
        traffic={traffic}
      />
    </>
  );
}
