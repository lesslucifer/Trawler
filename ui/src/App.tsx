import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { createApi, defaultProxyAddr, UNREACHABLE, type FlowView, type ReachState, type TrawlerApi } from "./api";
import { CA_FILES, hostOs, splitProxyAddr, type CaFormat } from "./setup";
import { NO_PROXY_READING, setUpOn, type ProxyReading, type TrustProbe } from "./verdicts";
import { saveBody } from "./body";
import { isDecryptedHttps, sessionTraffic } from "./flow";
import { useFlows, useFlowDetail } from "./useFlows";
import { useDraft } from "./useDraft";
import { useProxyCloseConfirm } from "./closeConfirm";
import { useAction } from "./useAction";
import { Layout } from "./Layout";
import { NotAnswering } from "./NotAnswering";
import { FlowList } from "./FlowList";
import { FlowDetail } from "./FlowDetail";
import { Composer } from "./Composer";
import { BackToFlows, Onboarding, useRouteMemory } from "./Onboarding";
import { ProxyCloseDialog } from "./ProxyCloseDialog";
import { CaptureControls } from "./CaptureControls";
import {
  Button,
  Field,
  HTTP_METHODS,
  MethodLabel,
  Notice,
  Select,
  StatusDot,
  type SelectOption,
} from "./components/index";
import {
  dotEvent,
  dotOpens,
  dotStatus,
  initial,
  QUIET_AFTER_MS,
  reduce,
  type OnbEvent,
  type OnbState,
} from "./onboardingMachine";

// A core on this machine answers /flows in milliseconds. One that has not
// answered the launch read in this long is silent, whether or not the read
// ever settles.
export const LAUNCH_PATIENCE_MS = 5_000;

// Select holds at most one option and never returns to unselected, so "any" is a
// real choice that happens to mean no filter — not a clear affordance the
// component's contract rules out.
const ANY = "\u0000any";

const METHOD_OPTIONS: SelectOption[] = [
  { id: ANY, label: "any method" },
  ...HTTP_METHODS.map((m) => ({
    id: m,
    label: m,
    element: <MethodLabel method={m} />,
  })),
];

// The detail pane shows one flow's inspector or the composer, in place of each
// other (epic decision 7): the list stays beside it, where a sent row lands.
type Pane = { kind: "inspector"; id: string } | { kind: "composer" } | null;

function hostOptions(hosts: string[]): SelectOption[] {
  return [{ id: ANY, label: "any host" }, ...hosts.map((h) => ({ id: h, label: h }))];
}

// The dot changes on its own once after a capture, when it goes quiet, so the
// shell re-renders at that edge rather than ticking.
function useNowPast(edge: number | null): number {
  const [, tick] = useState(0);
  useEffect(() => {
    if (edge === null) return;
    const left = edge - Date.now();
    if (left <= 0) return;
    const id = setTimeout(() => tick((n) => n + 1), left);
    return () => clearTimeout(id);
  }, [edge]);
  return Date.now();
}

export default function App({ api, initialState }: { api?: TrawlerApi; initialState?: OnbState }) {
  const client = useMemo(() => api ?? createApi(), [api]);
  const [state, dispatch] = useReducer(reduce, initialState, (s) => s ?? initial());
  const {
    filter,
    flows,
    everything,
    hosts,
    lastConnectionAt,
    lastCaptureAt,
    silent,
    answered,
    refused,
    retrying,
    retry,
    update,
  } = useFlows(client, state.phase === "list" ? "list" : "setup");
  // Which absence an empty list shows turns on WHY it is empty, and a clear is
  // the one reason no property of the list can carry. The flag disarms as soon
  // as rows return or the query moves, so it can only ever describe the emptiness
  // it caused.
  const [clearedEmpty, setClearedEmpty] = useState(false);
  const [pane, setPane] = useState<Pane>(null);
  const inspected = pane?.kind === "inspector" ? pane.id : null;
  const [retryNonce, setRetryNonce] = useState(0);
  const { flow: detail, failed: detailFailed } = useFlowDetail(client, inspected, retryNonce);
  const { kept, seedId, open: openDraft, edit, send } = useDraft(client);
  const inspect = useCallback((id: string) => setPane({ kind: "inspector", id }), []);
  const compose = (seed: FlowView | null) => {
    openDraft(seed);
    setPane({ kind: "composer" });
  };
  const leaveComposer = useCallback(() => {
    setPane(seedId ? { kind: "inspector", id: seedId } : null);
  }, [seedId]);

  // Launch self-test (W4): read /flows once and let live capture decide the entry
  // — a decrypted HTTPS flow already present means setup works, straight to the
  // list; otherwise show onboarding. Derived from capture, no persisted state.
  // A read nobody answered decides nothing: the app stays checking and says the
  // core is silent — setup steps are no answer to a core that is not running
  // (R2). Whatever the read's fate, any listing the core answered after it was
  // sent is the launch's answer, so the read keeps when it was sent and how many
  // listings had answered by then.
  const [launchUnanswered, setLaunchUnanswered] = useState(false);
  const answeredRef = useRef(answered);
  answeredRef.current = answered;
  const launchSent = useRef({ at: 0, answered: 0 });

  useEffect(() => {
    if (state.phase !== "checking") return;
    let live = true;
    const asked = Date.now();
    launchSent.current = { at: asked, answered: answeredRef.current };
    const patience = setTimeout(() => live && setLaunchUnanswered(true), LAUNCH_PATIENCE_MS);
    client.listFlows({}).then(
      (flows) =>
        live && dispatch(isDecryptedHttps(flows) ? { t: "captured" } : { t: "not-detected", at: asked }),
      () => live && setLaunchUnanswered(true),
    );
    return () => {
      live = false;
      clearTimeout(patience);
    };
  }, [client, state.phase]);

  // Live check (W4): the first decrypted HTTPS flow of this setup session reveals
  // the list. Gated on !everProven so setting up a second device stays in setup
  // and says so there; `everything` refetches on every SSE event via useFlows,
  // so observing it is the whole live check. A row from before the session
  // proves nothing about it (DEC-60), and the list is a click away (DEC-61), so
  // it moves no one out of a setup they asked for.
  // The moment setup succeeds used to be silent: the screen was replaced by a
  // list of one row and nothing said the thing the user had been working
  // towards had happened (F22). The banner says it on the list's first paint,
  // and only for the transition that IS that moment — a launch that finds
  // capture already working is not news.
  const [justSetUp, setJustSetUp] = useState(false);

  useEffect(() => {
    if (state.phase === "onboarding" && !state.everProven && isDecryptedHttps(everything, state.since ?? undefined)) {
      setJustSetUp(true);
      dispatch({ t: "captured" });
    }
  }, [state.phase, state.everProven, state.since, everything]);

  // Entering setup retires the announcement, however the user got there. The
  // banner belongs to one transition, and coming back out of setup without
  // having proven anything new is the "already working, not news" case — the
  // news would be a second success that did not happen.
  useEffect(() => {
    if (state.phase === "onboarding") setJustSetUp(false);
  }, [state.phase]);

  // Which absence the list shows turns on whether the user narrowed it, so an
  // empty-but-blank field must not count as an active filter.
  const hasFilter = Object.values(filter).some((v) => (v ?? "") !== "");

  const updateFilter = useCallback(
    (key: keyof typeof filter, value: string) => {
      setClearedEmpty(false);
      update(key, value);
    },
    [update],
  );

  useEffect(() => {
    if (flows.length > 0) setClearedEmpty(false);
  }, [flows]);

  const proxyAddr = defaultProxyAddr();

  // Local probes (W6): does *this machine* trust the CA / route its proxy through
  // Trawler? A probe is a snapshot, and nothing pushes when the user changes a
  // setting in System Settings — so it re-runs after an action, on "check again",
  // and whenever the window regains focus, and the reading carries its time.
  // Invoke isn't available in jsdom, so a failure leaves the probe "unknown".
  const [trustProbe, setTrustProbe] = useState<TrustProbe>("unknown");
  const [proxyReading, setProxyReading] = useState<ProxyReading>(NO_PROXY_READING);
  const [probing, setProbing] = useState(false);
  const [probedAt, setProbedAt] = useState<number | null>(null);
  const [probeNonce, setProbeNonce] = useState(0);
  const reprobe = useCallback(() => setProbeNonce((n) => n + 1), []);

  useEffect(() => {
    if (state.phase !== "onboarding") return;
    const { host, port } = splitProxyAddr(proxyAddr);
    let live = true;
    setProbing(true);
    Promise.allSettled([
      invoke<TrustProbe>("ca_trusted").then(
        (p) => live && setTrustProbe(p),
        () => live && setTrustProbe("unknown"),
      ),
      invoke<ProxyReading | undefined>("proxy_status", { host, port }).then(
        // A shell that answers with nothing has read nothing, which is what
        // "unknown" means — and the screen may not crash on the difference.
        (reading) => live && setProxyReading(reading ?? NO_PROXY_READING),
        () => live && setProxyReading(NO_PROXY_READING),
      ),
    ]).then(() => {
      if (!live) return;
      setProbing(false);
      setProbedAt(Date.now());
    });
    return () => {
      live = false;
    };
  }, [state.phase, proxyAddr, probeNonce]);

  useEffect(() => {
    if (state.phase !== "onboarding") return;
    window.addEventListener("focus", reprobe);
    return () => window.removeEventListener("focus", reprobe);
  }, [state.phase, reprobe]);

  // Reach is a fact about this process, never a stored preference (DEC-55): it
  // is read once the app has a core and re-read from whatever the core answers
  // to the two calls that change it. A read that fails leaves the screen saying
  // Trawler answers here only, which is the honest reading of a core that will
  // not say. It is read in every phase because an exposure this machine is
  // under is not a fact about the setup screen.
  const [reach, setReach] = useState<ReachState>(UNREACHABLE);
  const [reachNonce, setReachNonce] = useState(0);
  const rereadReach = useCallback(() => setReachNonce((n) => n + 1), []);

  // A core that restarts forgets an exposure, and one that started after the
  // window answers addresses the launch read never got (S3, S4). So reach is
  // re-read whenever the core comes back and whenever setup opens.
  const wasSilent = useRef(silent);
  useEffect(() => {
    if (wasSilent.current && !silent) rereadReach();
    wasSilent.current = silent;
  }, [silent, rereadReach]);

  const setupSession = state.phase === "onboarding" ? state.since : null;
  useEffect(() => {
    if (setupSession !== null) rereadReach();
  }, [setupSession, rereadReach]);

  useEffect(() => {
    let live = true;
    client.reach().then(
      (r) => live && setReach(r),
      () => live && setReach(UNREACHABLE),
    );
    return () => {
      live = false;
    };
  }, [client, reachNonce]);

  // The core answers with the kernel's device names, because asking the host
  // what its owner calls `en1` means running one of the host's own tools and
  // the core is kept out of that (DEC-13). The shell asks; a host with no
  // answer, and a browser with no shell, rename nothing.
  const [portNames, setPortNames] = useState<Record<string, string>>({});

  useEffect(() => {
    let live = true;
    invoke<Record<string, string>>("interface_names").then(
      (names) => live && setPortNames(names ?? {}),
      () => {},
    );
    return () => {
      live = false;
    };
  }, []);

  const named = useMemo(
    () => ({
      ...reach,
      interfaces: reach.interfaces.map((i) => ({ ...i, name: portNames[i.name] ?? i.name })),
    }),
    [reach, portNames],
  );

  // The confirm comes first and the bind only on a yes — the shell asks,
  // because arming changes what this machine exposes (DEC-13). A dialog that
  // cannot be shown is a question nobody answered, so nothing is armed — and
  // the refusal is returned rather than swallowed, because a screen that cannot
  // tell a no from a yes says nothing about either.
  const armReach = async (address: string) => {
    const allowed = await invoke<boolean>("confirm_reach", { address }).catch(() => false);
    if (!allowed) return null;
    setReach(await client.armReach(address));
    return undefined;
  };

  const disarmReach = async () => {
    setReach(await client.disarmReach());
  };

  const saveCa = async (format: CaFormat) => saveBody(await client.caFile(format), CA_FILES[format]);

  // Held here and not by the setup screen, so leaving setup does not lose it (S5).
  const [routeAct, setRouteAct] = useRouteMemory(proxyReading.status);

  // A proxy this machine already has is the user's, and replacing it is the
  // one setup action that takes something away — so it costs a confirm, the
  // same shape DEC-13 requires of every OS-touching change. A refusal resolves
  // "nothing happened" rather than a success, and writes nothing.
  const replaceAllowed = async () => {
    if (proxyReading.status !== "foreign") return true;
    return invoke<boolean>("confirm_replace_proxy", {
      address: proxyReading.address ?? proxyAddr,
    }).catch(() => false);
  };

  const setProxy = async () => {
    if (!(await replaceAllowed())) return null;
    const { host, port } = splitProxyAddr(proxyAddr);
    await invoke("set_proxy", { host, port });
    reprobe();
    return undefined;
  };

  // The one thing most users want, in one prompt: the shell runs both elevated
  // commands under a single authorization (F27).
  const setUpThisMachine = async () => {
    if (!(await replaceAllowed())) return null;
    const { host, port } = splitProxyAddr(proxyAddr);
    const pem = await client.caCertPem();
    await invoke("set_up_this_machine", { pem, host, port });
    reprobe();
    return undefined;
  };

  const unsetProxy = async () => {
    await invoke("unset_proxy");
    reprobe();
  };

  const close = useProxyCloseConfirm(proxyAddr);
  const closeDialog = close.mode && (
    <ProxyCloseDialog
      mode={close.mode}
      os={hostOs()}
      onRemove={close.remove}
      onKeep={close.keep}
      onAcknowledge={close.acknowledge}
      onCancel={close.cancel}
    />
  );

  // A move between the list and setup leaves focus on a control that no longer
  // exists, so the shell places it on the new screen's main area (S7). Only the
  // user's own moves claim it: a launch or a capture moving the screen may not
  // pull focus out of whatever the user was doing.
  const mainRef = useRef<HTMLElement>(null);
  const movedByUser = useRef(false);
  const go = (e: OnbEvent) => {
    if (reduce(state, e).phase !== state.phase) movedByUser.current = true;
    dispatch(e);
  };
  useEffect(() => {
    if (!movedByUser.current) return;
    movedByUser.current = false;
    mainRef.current?.focus();
  }, [state.phase]);

  const setUp = () => go({ t: "re-invoke", at: Date.now() });
  const setUpAnother = () => go({ t: "re-invoke", at: Date.now(), another: true });
  const now = useNowPast(lastCaptureAt == null ? null : lastCaptureAt + QUIET_AFTER_MS);
  const dot = (
    <StatusDot
      status={dotStatus(lastCaptureAt, now)}
      opens={dotOpens(state)}
      onClick={() => go(dotEvent(state, Date.now()))}
    />
  );

  const setUpBanner = justSetUp && (
    <div data-testid="set-up-banner">
      <Notice
        tone="ok"
        action={
          <>
            <Button onClick={setUpAnother}>set up another device</Button>
            <Button frame="bare" tone="mute" onClick={() => setJustSetUp(false)}>
              dismiss
            </Button>
          </>
        }
      >
        {setUpOn(null)}
      </Notice>
    </div>
  );

  // An exposure the user granted stays visible for as long as it holds — in the
  // list as much as in setup, where choosing "this machine" used to hide it
  // entirely. The chip carries the address a device is pointed at and the
  // control that ends it, because a warning nobody can act on is decoration.
  // The control runs through useAction for the reason Button's fork D4 states:
  // a stop that failed must not read like a stop that worked, and the exposure
  // it could not end is still on this machine.
  const disarm = useAction(disarmReach);
  const reachChip = reach.armed && reach.proxy !== null && (
    <div
      data-testid="reach-chip"
      className="flex items-center gap-control-gap rounded-button border border-hairline px-chrome-x font-ui text-chrome text-ink-mute"
    >
      <span>reachable on {splitProxyAddr(reach.proxy).host}</span>
      {disarm.trigger && (
        <Button
          frame="bare"
          tone="mute"
          onClick={disarm.trigger}
          disabled={disarm.state === "running"}
        >
          stop
        </Button>
      )}
      {disarm.state === "running" && <span>stopping…</span>}
      {disarm.state === "failed" && (
        <span className="text-warn" title={disarm.error}>
          couldn't stop listening
        </span>
      )}
    </div>
  );

  // The connection has one owner, written by both things that observe it — a
  // listing that did not arrive, and the stream going down — so the band and the
  // row region can never disagree about it. The pane's own failed fetch is a
  // third fact and stays the pane's: it can be true of a core answering
  // everything else, and a zone that borrows another zone's failure ends up
  // speaking for a question it was never asked.
  const notAnswering = silent;

  // Retry removes its own button, and Button leaves focus on nothing when it
  // stops existing — so the shell places it, keyed on the band actually leaving
  // rather than on which fetch cleared it. The list region is the target because
  // it exists whether or not the fresh listing has rows.
  const retried = useRef(false);
  const retryConnection = useCallback(() => {
    retried.current = true;
    setRetryNonce((n) => n + 1);
    return retry();
  }, [retry]);

  // The claim on focus lasts exactly one attempt: it is consumed when that
  // attempt settles, whether or not the band went with it. Left armed, a retry
  // that failed would still be waiting hours later to pull focus out of whatever
  // the user was doing when the core happened to come back on its own.
  useEffect(() => {
    if (!retried.current || retrying) return;
    retried.current = false;
    if (!notAnswering) mainRef.current?.focus();
  }, [notAnswering, retrying]);

  // The stream dropping during the launch read is the same silence as the read
  // failing, and the same listing answering ends both.
  useEffect(() => {
    if (state.phase === "checking" && silent) setLaunchUnanswered(true);
  }, [state.phase, silent]);

  useEffect(() => {
    if (state.phase !== "checking" || answered <= launchSent.current.answered) return;
    dispatch(isDecryptedHttps(everything) ? { t: "captured" } : { t: "not-detected", at: launchSent.current.at });
  }, [state.phase, answered, everything]);

  const setupHeader = (
    <>
      {dot}
      <h1 className="font-serif text-title italic">Trawler</h1>
      {reachChip}
    </>
  );

  // Until the launch read answers, the app does not know whether setup is
  // owed, so nothing it would instruct is drawn (R1).
  if (state.phase === "checking") {
    return (
      <Layout mainRef={mainRef} header={setupHeader}>
        {closeDialog}
        {launchUnanswered && (
          <div data-testid="empty" className="flex justify-center p-12">
            <NotAnswering retrying={retrying} onRetry={retryConnection} />
          </div>
        )}
      </Layout>
    );
  }

  // Every setup task reads this machine or the core, so a core that does not
  // answer leaves setup nothing true to say: the statement stands in place of
  // the steps, with the retry and the way back to the list (P5).
  if (state.phase === "onboarding" && notAnswering) {
    return (
      <Layout mainRef={mainRef} header={setupHeader}>
        {closeDialog}
        <div data-testid="empty" className="flex flex-col items-center gap-6 p-12">
          <NotAnswering trailing="setup needs it running" retrying={retrying} onRetry={retryConnection} />
          <BackToFlows onBack={() => go({ t: "exit" })} />
        </div>
      </Layout>
    );
  }

  if (state.phase === "onboarding") {
    return (
      <Layout mainRef={mainRef} header={setupHeader}>
        {closeDialog}
        <Onboarding
          state={state}
          dispatch={go}
          os={hostOs()}
          proxyAddr={proxyAddr}
          trustProbe={trustProbe}
          proxy={proxyReading}
          probing={probing}
          probedAt={probedAt}
          routeAct={routeAct}
          onRouteAct={setRouteAct}
          traffic={sessionTraffic(everything, state.since, lastConnectionAt)}
          onSetUpThisMachine={setUpThisMachine}
          onSaveCa={saveCa}
          onSetProxy={setProxy}
          onUnsetProxy={unsetProxy}
          onRecheck={reprobe}
          reach={named}
          onArmReach={armReach}
          onDisarmReach={disarmReach}
          onRereadReach={rereadReach}
        />
      </Layout>
    );
  }

  return (
    <Layout
      mainRef={mainRef}
      band={
        // The statement sits immediately above what it is about. With rows, that
        // is the rows; with a refusal standing in their place, it is the refusal
        // — the core answered that query and then stopped answering, and both
        // are true. With neither, there is nothing above to qualify and the
        // sentence moves into the row region instead. A core that stopped
        // answering outranks the good news: the band states one thing, and the
        // banner is the one a user can come back to.
        notAnswering && (flows.length > 0 || refused !== null) ? (
          <NotAnswering
            trailing={flows.length > 0 ? "the rows below are the last it sent" : undefined}
            retrying={retrying}
            onRetry={retryConnection}
          />
        ) : (
          setUpBanner || undefined
        )
      }
      header={
        <>
          {dot}
          <h1 className="mr-2 font-serif text-title italic">Trawler</h1>
          {reachChip}
          <Field
            aria-label="filter"
            placeholder="filter — free text"
            className="min-w-field flex-1"
            value={filter.q ?? ""}
            onChange={(e) => updateFilter("q", e.target.value)}
          />
          <Select
            label="host"
            placeholder="any host"
            className="w-field-lg"
            options={hostOptions(hosts)}
            value={filter.host || ANY}
            onChange={(id) => updateFilter("host", id === ANY ? "" : id)}
          />
          <Select
            label="method"
            placeholder="any method"
            className="w-field-lg"
            searchable={false}
            options={METHOD_OPTIONS}
            value={filter.method || ANY}
            onChange={(id) => updateFilter("method", id === ANY ? "" : id)}
          />
          {/* Free text, on its second and durable leg: a Select can offer only
              values capture has already seen, and filtering for a 404 before one
              arrives is the debugging act. The set is not knowable, so no closed
              control serves this key — and no keypad hint either, now that a
              class is a letter. */}
          <Field
            aria-label="status"
            placeholder="status — 404 or 4xx"
            className="mr-2 w-field"
            value={filter.status ?? ""}
            onChange={(e) => updateFilter("status", e.target.value)}
          />
          <CaptureControls
            api={client}
            filter={filter}
            count={flows.length}
            onCleared={() => setClearedEmpty(true)}
            onNewRequest={() => compose(null)}
          />
        </>
      }
      detail={
        pane?.kind === "composer" && kept ? (
          <Composer
            key={kept.serial}
            draft={kept.draft}
            outcome={kept.outcome}
            onEdit={edit}
            onSend={() => void send()}
            onClose={leaveComposer}
            onOpenFlow={inspect}
          />
        ) : inspected != null ? (
          <FlowDetail
            key={inspected}
            api={client}
            flow={detail}
            failed={detailFailed}
            onClose={() => setPane(null)}
            onReplay={detail ? () => compose(detail) : undefined}
          />
        ) : undefined
      }
    >
      {closeDialog}
      <FlowList
        flows={flows}
        selectedId={inspected ?? (pane?.kind === "composer" ? seedId : null)}
        onSelect={inspect}
        filtered={hasFilter}
        cleared={clearedEmpty}
        silent={silent}
        refused={refused}
        retrying={retrying}
        onRetry={retryConnection}
        onSetUp={setUp}
      />
    </Layout>
  );
}
