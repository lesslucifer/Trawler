import {
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { downloadDir } from "@tauri-apps/api/path";
import {
  createApi,
  defaultProxyAddr,
  UNREACHABLE,
  type FlowView,
  type ReachState,
  type TrawlerApi,
} from "./api";
import {
  CA_FILES,
  hostOs,
  lostAddress,
  marksItsOwn,
  needsReach,
  readsOverAdb,
  sharesHostWire,
  splitProxyAddr,
  type CaFormat,
  type RouteName,
  type SavedCa,
} from "./setup";
import {
  mayCarryOwnTraffic,
  NO_PROXY_READING,
  setUpOn,
  TUNNEL_GRACE_MS,
  type EmulatorRead,
  type ProxyReading,
  type SimulatorTrust,
  type TrustProbe,
} from "./verdicts";
import { isFullPath, saveBody } from "./body";
import { useNotesShown } from "./noticePreference";
import { newestRead } from "./newestRead";
import { isDecryptedHttps, sessionTraffic } from "./flow";
import { useFlows, useFlowDetail } from "./useFlows";
import { useDraft } from "./useDraft";
import { bounded, useProxyCloseConfirm, type QuitFacts } from "./closeConfirm";
import { useAction } from "./useAction";
import { Layout } from "./Layout";
import { useLayout } from "./useLayout";
import { NotAnswering } from "./NotAnswering";
import { FlowList } from "./FlowList";
import { FlowDetail } from "./FlowDetail";
import { Composer } from "./Composer";
import { BackToFlows, Onboarding, useRouteMemory } from "./Onboarding";
import { ProxyCloseDialog } from "./ProxyCloseDialog";
import { CaptureControls } from "./CaptureControls";
import {
  deviceEvidenceSince,
  deviceMark,
  deviceTraffic,
  isOwnRequest,
  makeRequest,
  nextArmedSince,
  nextClearReadings,
  nextMark,
  NO_CLEAR_READINGS,
  ownRequestProvedAt,
  REQUEST_HOST,
  requestRoute,
  type ArmedSince,
  type ClearReadings,
  type RequestRoute,
} from "./ownRequest";
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

// How often setup re-reads this machine while it is on screen (U3 · DEC-62):
// a setting changed in System Settings beside the window shows within it.
export const WATCH_MS = 3_000;
// The certificate each machine read asks about comes from the core, and the read
// lands only once every part has: a core that took the request and never
// answers may not hold the readings of this machine's own settings with it.
export const CA_READ_PATIENCE_MS = 5_000;
// Two readings of this machine's proxy further apart than this had a hidden
// window or a closed setup between them, and vouch for nothing in that gap
// (DEC-63 rule 3); a quiet read that waited out three ticks in flight still
// joins its run.
const READINGS_APART_MAX_MS = 4 * WATCH_MS;

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

// The watch re-reads reach every few seconds, and an answer the screen already
// holds may move nothing on it (U4).
const sameReach = (a: ReachState, b: ReachState) =>
  JSON.stringify(a) === JSON.stringify(b);

function hostOptions(hosts: string[]): SelectOption[] {
  return [
    { id: ANY, label: "any host" },
    ...hosts.map((h) => ({ id: h, label: h })),
  ];
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

export default function App({
  api,
  initialState,
}: {
  api?: TrawlerApi;
  initialState?: OnbState;
}) {
  const client = useMemo(() => api ?? createApi(), [api]);
  const [state, dispatch] = useReducer(
    reduce,
    initialState,
    (s) => s ?? initial(),
  );
  const {
    filter,
    flows,
    everything,
    hosts,
    tunnelTimes,
    tunnelAt,
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
  const split = useLayout();
  const inspected = pane?.kind === "inspector" ? pane.id : null;
  const [retryNonce, setRetryNonce] = useState(0);
  const { flow: detail, failed: detailFailed } = useFlowDetail(
    client,
    inspected,
    retryNonce,
  );
  const { kept, seedId, open: openDraft, edit, send } = useDraft(client);
  const inspect = useCallback(
    (id: string) => setPane({ kind: "inspector", id }),
    [],
  );
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
    const patience = setTimeout(
      () => live && setLaunchUnanswered(true),
      LAUNCH_PATIENCE_MS,
    );
    client.listFlows({}).then(
      (flows) =>
        live &&
        dispatch(
          isDecryptedHttps(flows)
            ? { t: "captured" }
            : { t: "not-detected", at: asked },
        ),
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
    if (
      state.phase === "onboarding" &&
      !state.everProven &&
      isDecryptedHttps(everything, state.since ?? undefined)
    ) {
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
  // setting in System Settings — so it is read again wherever its answer can
  // change (DEC-62): as setup opens, after every action whatever its outcome,
  // when the window regains focus, when the core comes back
  // serving a certificate that may be new, and on a watch while setup shows. A
  // read the user asked for shows itself; one they did not is quiet, and moves
  // only the time the reading carries (U4). Only the newest read may land.
  // Invoke isn't available in jsdom, so a failure leaves the probe "unknown".
  const [trustProbe, setTrustProbe] = useState<TrustProbe>("unknown");
  const [proxyReading, setProxyReading] =
    useState<ProxyReading>(NO_PROXY_READING);
  const [simulators, setSimulators] = useState<
    readonly SimulatorTrust[] | null
  >(null);
  // No reading until one lands: adb not found is no reading either, and none
  // of it says no emulator runs (W17's W4).
  const [emulatorRead, setEmulatorRead] = useState<EmulatorRead | null>(null);
  // adb starts a server of its own where none runs, so the emulator is read
  // only while it is the platform chosen (W8's Q1).
  const readsEmulators = useRef(false);
  readsEmulators.current = readsOverAdb(state.chosen);
  const [probing, setProbing] = useState(false);
  // When this machine's proxy read off or another, so nothing of its own could
  // be arriving: another device's unmarked evidence is only what arrived then
  // (DEC-63 rule 3), and whatever the reading does later leaves it there.
  const [clearReadings, setClearReadings] =
    useState<ClearReadings>(NO_CLEAR_READINGS);
  const [machineReads] = useState(newestRead);

  const readMachine = useCallback(
    (mode: "shown" | "quiet") => {
      const landing = machineReads.begin(mode);
      if (landing === null) return;
      const sent = Date.now();
      if (mode === "shown") setProbing(true);
      const { host, port } = splitProxyAddr(proxyAddr);
      // The certificate the core serves now is the one asked about: a store
      // holding an older one under the same name answers nothing (D1). Each
      // running Simulator's own store is asked about it too (W7's Q1).
      const pem = bounded<string | null>(
        client.caCertPem(),
        null,
        CA_READ_PATIENCE_MS,
      );
      const trust = pem
        .then((live) =>
          live === null
            ? undefined
            : invoke<TrustProbe | undefined>("ca_trusted", { pem: live }),
        )
        .then(
          (p): TrustProbe => p ?? "unknown",
          (): TrustProbe => "unknown",
        );
      const running = pem
        .then((live) =>
          live === null
            ? undefined
            : invoke<SimulatorTrust[] | undefined>("simulator_trust", {
                pem: live,
              }),
        )
        .then(
          (read): readonly SimulatorTrust[] | null => read ?? null,
          (): null => null,
        );
      const adb = readsEmulators.current
        ? pem
            .then((live) =>
              live === null
                ? undefined
                : invoke<EmulatorRead | undefined>("emulator_readings", {
                    pem: live,
                    port,
                  }),
            )
            .then(
              (read): EmulatorRead | null => read ?? null,
              (): null => null,
            )
        : Promise.resolve(null);
      // A shell that answers with nothing has read nothing, which is what
      // "unknown" means — and the screen may not crash on the difference.
      const route = invoke<ProxyReading | undefined>("proxy_in_use", {
        host,
        port,
      }).then(
        (reading) => reading ?? NO_PROXY_READING,
        () => NO_PROXY_READING,
      );
      void Promise.all([trust, route, running, adb]).then(
        ([t, r, sims, emus]) => {
          if (!landing()) return;
          const landed = Date.now();
          setTrustProbe(t);
          setProxyReading(r);
          setSimulators((held) =>
            JSON.stringify(held) === JSON.stringify(sims) ? held : sims,
          );
          setEmulatorRead((held) =>
            JSON.stringify(held) === JSON.stringify(emus) ? held : emus,
          );
          setClearReadings((was) =>
            nextClearReadings(
              was,
              !mayCarryOwnTraffic(r.status),
              { sent, landed },
              READINGS_APART_MAX_MS,
            ),
          );
          setProbing(false);
        },
      );
    },
    [client, proxyAddr, machineReads],
  );

  // Reach is a fact about this process, never a stored preference (DEC-55): it
  // is read once the app has a core and re-read from whatever the core answers
  // to the two calls that change it. A read that fails leaves the screen saying
  // Trawler answers here only, which is the honest reading of a core that will
  // not say. It is read in every phase because an exposure this machine is
  // under is not a fact about the setup screen.
  const [reach, setReach] = useState<ReachState>(UNREACHABLE);

  const [armedSince, setArmedSince] = useState<ArmedSince>(null);
  const takeReach = useCallback((next: ReachState) => {
    setReach((held) => (sameReach(held, next) ? held : next));
    setArmedSince((was) => nextArmedSince(was, next, Date.now()));
  }, []);

  // An allow's and a stop's own answer overtakes any read sent before it. A
  // read that failed armed nothing anew, so it never moves the evidence clock
  // (DEC-63 rule 1); and one nobody asked for says nothing at all - a core that
  // stopped answering is the silent band's to say, not a reason for the tasks
  // to flicker.
  const [reachReads] = useState(newestRead);
  const readReach = useCallback(
    (mode: "shown" | "quiet") => {
      const landing = reachReads.begin(mode);
      if (landing === null) return;
      client.reach().then(
        (answer) => landing() && takeReach(answer),
        () => landing() && mode === "shown" && setReach(UNREACHABLE),
      );
    },
    [client, takeReach, reachReads],
  );
  const rereadReach = useCallback(() => readReach("shown"), [readReach]);
  const landReach = (answer: ReachState) => {
    reachReads.supersede();
    takeReach(answer);
  };

  // Whatever can change this machine's answers can change its addresses too,
  // so the reads nobody asked for take both (R2).
  const rereadQuietly = useCallback(() => {
    readMachine("quiet");
    readReach("quiet");
  }, [readMachine, readReach]);

  const inSetup = state.phase === "onboarding";
  useEffect(() => {
    if (inSetup) readMachine("shown");
  }, [inSetup, readMachine]);

  const choseEmulator = inSetup && readsOverAdb(state.chosen);
  useEffect(() => {
    if (choseEmulator) readMachine("shown");
  }, [choseEmulator, readMachine]);

  useEffect(() => {
    if (!inSetup) return;
    window.addEventListener("focus", rereadQuietly);
    return () => window.removeEventListener("focus", rereadQuietly);
  }, [inSetup, rereadQuietly]);

  // U3: the watch runs while setup is on screen, its core answering, and the
  // window visible, and nowhere else: each tick is a few shell commands.
  const watching = inSetup && !silent;
  useEffect(() => {
    if (!watching) return;
    let timer: ReturnType<typeof setInterval> | undefined;
    const start = () => {
      timer ??= setInterval(rereadQuietly, WATCH_MS);
    };
    const stop = () => {
      clearInterval(timer);
      timer = undefined;
    };
    const onVisibility = () => {
      if (document.visibilityState !== "visible") return stop();
      rereadQuietly();
      start();
    };
    if (document.visibilityState === "visible") start();
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      stop();
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [watching, rereadQuietly]);

  // A core that restarts forgets an exposure, and one that started after the
  // window answers addresses the launch read never got (S3, S4). So reach is
  // re-read whenever the core comes back and whenever setup opens. So is this
  // machine, in setup: a core started again may serve a new certificate (U2).
  const wasSilent = useRef(silent);
  useEffect(() => {
    if (wasSilent.current && !silent) {
      rereadReach();
      if (inSetup) readMachine("quiet");
    }
    wasSilent.current = silent;
  }, [silent, rereadReach, inSetup, readMachine]);

  const setupSession = state.phase === "onboarding" ? state.since : null;
  useEffect(() => {
    if (setupSession !== null) rereadReach();
  }, [setupSession, rereadReach]);

  useEffect(() => {
    rereadReach();
  }, [rereadReach]);

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
      interfaces: reach.interfaces.map((i) => ({
        ...i,
        name: portNames[i.name] ?? i.name,
      })),
    }),
    [reach, portNames],
  );

  const ownProofAt = useMemo(
    () =>
      state.phase === "onboarding"
        ? ownRequestProvedAt(everything, state.since)
        : null,
    [state.phase, everything, state.since],
  );

  // This machine's evidence is the session's. Another device's starts when the
  // address it uses was allowed (A2), and is what could have come from it (DEC-63).
  // The Simulator's wire is this machine's own, so no reading of this machine's
  // proxy clears it: from the session's start, only its mark is its own (W7).
  // The emulator needs no allow - it reaches this machine's loopback - so its
  // evidence starts with the session (W8).
  const sharedWire = sharesHostWire(state.chosen);
  const deviceSince = needsReach(state.chosen)
    ? deviceEvidenceSince(state.since, armedSince)
    : state.since;
  const deviceClear = sharedWire ? NO_CLEAR_READINGS : clearReadings;
  const mark = marksItsOwn(state.chosen)
    ? deviceMark(state.since, state.chosen)
    : undefined;
  const device = useMemo(
    () =>
      state.phase === "onboarding" && mark !== undefined
        ? deviceTraffic(
            everything,
            { allowedSince: deviceSince, mark, clear: deviceClear },
            tunnelTimes,
            tunnelAt(REQUEST_HOST),
          )
        : undefined,
    [
      state.phase,
      everything,
      mark,
      deviceSince,
      deviceClear,
      tunnelTimes,
      tunnelAt,
    ],
  );

  // The confirm comes first and the bind only on a yes — the shell asks,
  // because arming changes what this machine exposes (DEC-13). A dialog that
  // cannot be shown is a question nobody answered, so nothing is armed — and
  // the refusal is returned rather than swallowed, because a screen that cannot
  // tell a no from a yes says nothing about either.
  const armReach = async (address: string) => {
    const allowed = await invoke<boolean>("confirm_reach", { address }).catch(
      () => false,
    );
    if (!allowed) return null;
    landReach(await client.armReach(address));
    return undefined;
  };

  const disarmReach = async () => {
    landReach(await client.disarmReach());
  };

  // Where the save put each file, held here so a command names it after setup
  // closes and opens again (W7's C1). The dialog opens in Downloads, where the
  // commands look until a save says otherwise; a browser's download names no
  // folder, so it moves nothing.
  const [savedCa, setSavedCa] = useState<SavedCa>({});
  const saveCa = async (format: CaFormat) => {
    const path = await saveBody(
      await client.caFile(format),
      CA_FILES[format],
      downloadDir,
    );
    if (path !== null && isFullPath(path))
      setSavedCa((was) => ({ ...was, [format]: path }));
    return path;
  };

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

  // U1: every outcome re-reads, a failure's too. A write can fail half done —
  // -60007 after the trust was written, a store written and a proxy not — and
  // a cancelled one may still have run a step before the prompt it stopped at.
  const thenRead =
    <T,>(act: () => Promise<T>) =>
    async (): Promise<T> => {
      try {
        return await act();
      } finally {
        readMachine("shown");
      }
    };

  const setProxy = thenRead(async () => {
    if (!(await replaceAllowed())) return null;
    const { host, port } = splitProxyAddr(proxyAddr);
    await invoke("set_proxy", { host, port });
    return undefined;
  });

  // The one thing most users want, in one prompt: the shell runs both elevated
  // commands under a single authorization (F27).
  const setUpThisMachine = thenRead(async () => {
    if (!(await replaceAllowed())) return null;
    const { host, port } = splitProxyAddr(proxyAddr);
    const pem = await client.caCertPem();
    await invoke("set_up_this_machine", { pem, host, port });
    return undefined;
  });

  // simctl writes every running Simulator's store with no prompt, so the trust
  // goes first: a proxy is never left routing a Simulator that does not trust it.
  const trustSimulators = thenRead(async () => {
    await invoke("trust_simulators", { pem: await client.caCertPem() });
  });

  const setUpSimulator = thenRead(async () => {
    if (!(await replaceAllowed())) return null;
    await invoke("trust_simulators", { pem: await client.caCertPem() });
    const { host, port } = splitProxyAddr(proxyAddr);
    await invoke("set_proxy", { host, port });
    return undefined;
  });

  // su writes every running emulator's store with no prompt, so the install
  // goes first: no emulator is left routing through a proxy it does not trust.
  const installOnEmulators = thenRead(async () => {
    await invoke("install_on_emulators", { pem: await client.caCertPem() });
  });

  const routeEmulators = thenRead(async () => {
    await invoke("route_emulators", { port: splitProxyAddr(proxyAddr).port });
  });

  const unrouteEmulators = thenRead(async () => {
    await invoke("unroute_emulators");
  });

  const setUpEmulator = thenRead(async () => {
    await invoke("install_on_emulators", { pem: await client.caCertPem() });
    await invoke("route_emulators", { port: splitProxyAddr(proxyAddr).port });
  });

  const unsetProxy = thenRead(async () => {
    await invoke("unset_proxy");
  });

  const removeCa = thenRead(async () => {
    await invoke("remove_ca", { pem: await client.caCertPem() });
  });

  // A by-hand step's route: the shell opens it by name, and builds what it
  // opens. The browser's request carries this window's mark, so it settles the
  // tasks as the in-app one does (DEC-62).
  const openRoute = async (route: RouteName) => {
    if (
      route === "keychain" ||
      route === "certificate" ||
      route === "emulator-certificate"
    ) {
      await invoke("open_route", {
        route: { name: route, pem: await client.caCertPem() },
      });
    } else if (route === "browser") {
      await invoke("open_route", { route: { name: route, mark: nextMark() } });
    } else if (route === "simulator-request" || route === "emulator-request") {
      await invoke("open_route", { route: { name: route, mark } });
    } else {
      await invoke("open_route", { route: { name: route } });
    }
  };

  // D9: which way this window's own request went is what the core saw of it -
  // its marked row decrypted, a tunnel to its host and no row, or nothing - and
  // the core's account lands a listing after the page's answer, so it is
  // awaited for as long as a tunnel's grace.
  const awaitingOwn = useRef<{ mark: string; seen: () => void } | null>(null);
  useEffect(() => {
    const waiting = awaitingOwn.current;
    if (
      waiting !== null &&
      everything.some((f) => isOwnRequest(f, waiting.mark))
    )
      waiting.seen();
  }, [everything]);

  const makeOwnRequest = async (): Promise<RequestRoute> => {
    const mark = nextMark();
    const sentAt = Date.now();
    const seen = new Promise<boolean>((resolve) => {
      awaitingOwn.current = { mark, seen: () => resolve(true) };
    });
    const answered = await makeRequest(mark).then(
      () => true,
      () => false,
    );
    const grace = new Promise<boolean>((resolve) =>
      setTimeout(() => resolve(false), TUNNEL_GRACE_MS),
    );
    const decrypted = await Promise.race([seen, grace]);
    if (awaitingOwn.current?.mark === mark) awaitingOwn.current = null;
    const route = requestRoute({
      decrypted,
      tunnelAt: tunnelAt(REQUEST_HOST),
      sentAt,
      answered,
    });
    if (route === null)
      throw new Error(`the request to ${REQUEST_HOST} didn't complete`);
    return route;
  };

  // Read at the moment of the close, not from what the screen last held: the
  // emulator is asked through adb only while it is the platform chosen, since
  // adb starts a server of its own (W8's Q1).
  // A Wi-Fi-only proxy sits on a network the emulator does not send over, so it
  // routes nothing. A capture service that has stopped answering still ran on
  // this machine's addresses, so the last ones it reported stand in for them.
  const chosenAtQuit = useRef(state.chosen);
  chosenAtQuit.current = state.chosen;
  const reachAtQuit = useRef(reach);
  reachAtQuit.current = reach;
  const readQuitFacts = useCallback(async (): Promise<QuitFacts> => {
    const emulatorRead = readsOverAdb(chosenAtQuit.current)
      ? client
          .caCertPem()
          .then((pem) =>
            invoke<EmulatorRead | undefined>("emulator_readings", {
              pem,
              port: splitProxyAddr(proxyAddr).port,
            }),
          )
          .then(
            (read) =>
              read?.emulators.find((e) => e.proxy === "ours")?.name ?? null,
          )
      : Promise.resolve(null);
    const [reached, emulator] = await Promise.all([
      bounded<ReachState | null>(client.reach(), null),
      bounded<string | null>(emulatorRead, null),
    ]);
    return {
      answering: reached !== null,
      ownAddresses: (reached ?? reachAtQuit.current).interfaces.map(
        (i) => i.address,
      ),
      reachArmed: reached?.armed ?? false,
      emulator,
    };
  }, [client, proxyAddr]);
  const close = useProxyCloseConfirm(proxyAddr, readQuitFacts);
  const [notesShown, setNotesShown] = useNotesShown();
  const closeDialog = close.prompt && (
    <ProxyCloseDialog
      prompt={close.prompt}
      os={hostOs()}
      chosen={state.chosen}
      onStop={close.stop}
      onQuit={close.quit}
      onCancel={close.cancel}
      notesShown={notesShown}
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
  const setUpAnother = () =>
    go({ t: "re-invoke", at: Date.now(), another: true });
  const now = useNowPast(
    lastCaptureAt == null ? null : lastCaptureAt + QUIET_AFTER_MS,
  );
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
            <Button
              frame="bare"
              tone="mute"
              onClick={() => setJustSetUp(false)}
            >
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
  // An address this machine has left is still listened on and reaches no one,
  // and the chip says so as step 1 does (R1).
  // The control runs through useAction for the reason Button's fork D4 states:
  // a stop that failed must not read like a stop that worked, and the exposure
  // it could not end is still on this machine.
  const disarm = useAction(disarmReach);
  const lost = lostAddress(reach);
  const reachChip = reach.armed && reach.proxy !== null && (
    <div
      data-testid="reach-chip"
      className="flex items-center gap-control-gap rounded-button border border-hairline px-chrome-x font-ui text-chrome text-ink-mute"
    >
      <span>
        {lost === null
          ? `reachable on ${splitProxyAddr(reach.proxy).host}`
          : `no longer reachable on ${lost}`}
      </span>
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
    if (state.phase !== "checking" || answered <= launchSent.current.answered)
      return;
    dispatch(
      isDecryptedHttps(everything)
        ? { t: "captured" }
        : { t: "not-detected", at: launchSent.current.at },
    );
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
          <div
            data-testid="empty"
            className="flex h-full items-center justify-center p-12"
          >
            <NotAnswering
              retrying={retrying}
              onRetry={retryConnection}
              region
            />
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
        <div
          data-testid="empty"
          className="flex h-full flex-col items-center justify-center gap-6 p-12"
        >
          <NotAnswering
            trailing="Setup can't check this computer or see your requests until it's back."
            retrying={retrying}
            onRetry={retryConnection}
            region
          />
          <BackToFlows onBack={() => go({ t: "exit" })} />
        </div>
      </Layout>
    );
  }

  if (state.phase === "onboarding") {
    const traffic =
      device?.traffic ?? sessionTraffic(everything, state.since, tunnelTimes);
    return (
      <Layout mainRef={mainRef} header={setupHeader}>
        {closeDialog}
        <Onboarding
          state={state}
          dispatch={go}
          notesShown={notesShown}
          onShowNotes={setNotesShown}
          os={hostOs()}
          proxyAddr={proxyAddr}
          trustProbe={trustProbe}
          proxy={proxyReading}
          probing={probing}
          routeAct={routeAct}
          onRouteAct={setRouteAct}
          traffic={traffic}
          ownProofAt={ownProofAt}
          simulators={simulators}
          emulators={emulatorRead?.adb ? emulatorRead.emulators : null}
          androidSdk={emulatorRead}
          emulatorStarting={emulatorRead?.starting ?? false}
          savedCa={savedCa}
          deviceMark={mark}
          device={device}
          onSetUpThisMachine={setUpThisMachine}
          onSetUpSimulator={setUpSimulator}
          onTrustSimulators={trustSimulators}
          onSetUpEmulator={setUpEmulator}
          onInstallOnEmulators={installOnEmulators}
          onRouteEmulators={routeEmulators}
          onUnrouteEmulators={unrouteEmulators}
          onSaveCa={saveCa}
          onSetProxy={setProxy}
          onUnsetProxy={unsetProxy}
          onRemoveCa={removeCa}
          reach={named}
          onArmReach={armReach}
          onDisarmReach={disarmReach}
          onRereadReach={rereadReach}
          onMakeRequest={makeOwnRequest}
          onOpenRoute={openRoute}
        />
      </Layout>
    );
  }

  return (
    <Layout
      mainRef={mainRef}
      split={split}
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
            trailing={
              flows.length > 0
                ? "The requests below are the last it sent. New ones will show up once it's back."
                : undefined
            }
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
            onSetUpDevice={setUpAnother}
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
            split={split}
          />
        ) : undefined
      }
    >
      {closeDialog}
      <FlowList
        split={split}
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
