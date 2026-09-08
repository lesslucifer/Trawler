import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { createApi, defaultProxyAddr, type TrawlerApi } from "./api";
import { CA_FILENAME, hostOs, splitProxyAddr, type ProxyProbe, type TrustProbe } from "./setup";
import { saveBody } from "./body";
import { isDecryptedHttps, latestFlowTimes } from "./flow";
import { useFlows, useFlowDetail } from "./useFlows";
import { useProxyCloseConfirm } from "./closeConfirm";
import { Layout } from "./Layout";
import { NotAnswering } from "./NotAnswering";
import { FlowList } from "./FlowList";
import { FlowDetail } from "./FlowDetail";
import { Onboarding } from "./Onboarding";
import { ProxyCloseDialog } from "./ProxyCloseDialog";
import {
  Field,
  HTTP_METHODS,
  MethodLabel,
  Select,
  StatusDot,
  TonedText,
  type SelectOption,
} from "./components/index";
import { dotStatus, initial, reduce, type OnbState } from "./onboardingMachine";

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

function hostOptions(hosts: string[]): SelectOption[] {
  return [{ id: ANY, label: "any host" }, ...hosts.map((h) => ({ id: h, label: h }))];
}

export default function App({ api, initialState }: { api?: TrawlerApi; initialState?: OnbState }) {
  const client = useMemo(() => api ?? createApi(), [api]);
  const { filter, flows, hosts, connections, lastConnectionAt, silent, refused, retrying, retry, update } =
    useFlows(client);
  const [state, dispatch] = useReducer(reduce, initialState, (s) => s ?? initial());
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [retryNonce, setRetryNonce] = useState(0);
  const { flow: detail, failed: detailFailed } = useFlowDetail(client, selectedId, retryNonce);

  // Launch self-test (W4): read /flows once and let live capture decide the entry
  // — a decrypted HTTPS flow already present means setup works, straight to the
  // list; otherwise show onboarding. Derived from capture, no persisted state.
  useEffect(() => {
    if (state.phase !== "checking") return;
    let live = true;
    client
      .listFlows({})
      .then((flows) => live && dispatch(isDecryptedHttps(flows) ? { t: "captured" } : { t: "not-detected" }))
      .catch(() => live && dispatch({ t: "not-detected" }));
    return () => {
      live = false;
    };
  }, [client, state.phase]);

  // Live check (W4): the first-ever decrypted HTTPS flow the stream brings in flips
  // Verify green and reveals the list. Gated on !everProven so a deliberate
  // re-invoke (help) isn't instantly bounced back by the already-captured flow;
  // once proven, the dot and Back-to-flows drive navigation. `flows` refetches on
  // every SSE event via useFlows, so observing it is the whole live check.
  useEffect(() => {
    if (state.phase === "onboarding" && !state.everProven && isDecryptedHttps(flows)) {
      dispatch({ t: "captured" });
    }
  }, [state.phase, state.everProven, flows]);

  // Which absence the list shows turns on whether the user narrowed it, so an
  // empty-but-blank field must not count as an active filter.
  const hasFilter = Object.values(filter).some((v) => (v ?? "") !== "");

  const proxyAddr = defaultProxyAddr();

  // Local probes (W6): does *this machine* trust the CA / route its proxy through
  // Trawler? A probe is a snapshot, and nothing pushes when the user changes a
  // setting in System Settings — so it re-runs after an action, on "check again",
  // and whenever the window regains focus, and the reading carries its time.
  // Invoke isn't available in jsdom, so a failure leaves the probe "unknown".
  const [trustProbe, setTrustProbe] = useState<TrustProbe>("unknown");
  const [proxyProbe, setProxyProbe] = useState<ProxyProbe>("unknown");
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
      invoke<ProxyProbe>("proxy_status", { host, port }).then(
        (p) => live && setProxyProbe(p),
        () => live && setProxyProbe("unknown"),
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

  const installCa = async () => {
    const pem = await client.caCertPem();
    await invoke("install_ca", { pem });
    reprobe();
  };

  const saveCa = async () => {
    const pem = await client.caCertPem();
    return saveBody(new TextEncoder().encode(pem), CA_FILENAME);
  };

  const setProxy = async () => {
    const { host, port } = splitProxyAddr(proxyAddr);
    await invoke("set_proxy", { host, port });
    reprobe();
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

  const dot = <StatusDot status={dotStatus(state)} onClick={() => dispatch({ t: "re-invoke" })} />;

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
  const mainRef = useRef<HTMLElement>(null);
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

  if (state.phase !== "proven") {
    return (
      <Layout
        header={
          <>
            {dot}
            <h1 className="font-serif text-title italic">Trawler</h1>
          </>
        }
      >
        {closeDialog}
        <Onboarding
          state={state}
          dispatch={dispatch}
          os={hostOs()}
          proxyAddr={proxyAddr}
          trustProbe={trustProbe}
          proxyProbe={proxyProbe}
          probing={probing}
          probedAt={probedAt}
          traffic={{
            ...latestFlowTimes(flows),
            lastTunnelAt: lastConnectionAt,
            ever: flows.length > 0 || connections > 0,
          }}
          onInstallCa={installCa}
          onSaveCa={saveCa}
          onSetProxy={setProxy}
          onUnsetProxy={unsetProxy}
          onRecheck={reprobe}
        />
      </Layout>
    );
  }

  return (
    <Layout
      mainRef={mainRef}
      band={
        // Only above rows: with none, the same sentence stands in the row region
        // instead, so the failure is stated immediately above what it is about
        // and never twice.
        notAnswering && flows.length > 0 ? (
          <NotAnswering
            trailing="the rows below are the last it sent"
            retrying={retrying}
            onRetry={retryConnection}
          />
        ) : undefined
      }
      header={
        <>
          {dot}
          <h1 className="mr-2 font-serif text-title italic">Trawler</h1>
          <Field
            aria-label="filter"
            placeholder="filter — free text"
            className="min-w-field flex-1"
            value={filter.q ?? ""}
            onChange={(e) => update("q", e.target.value)}
          />
          <Select
            label="host"
            placeholder="any host"
            className="w-field-lg"
            options={hostOptions(hosts)}
            value={filter.host || ANY}
            onChange={(id) => update("host", id === ANY ? "" : id)}
          />
          <Select
            label="method"
            placeholder="any method"
            className="w-field-lg"
            searchable={false}
            options={METHOD_OPTIONS}
            value={filter.method || ANY}
            onChange={(id) => update("method", id === ANY ? "" : id)}
          />
          {/* Free text, on its second and durable leg: a Select can offer only
              values capture has already seen, and filtering for a 404 before one
              arrives is the debugging act. The set is not knowable, so no closed
              control serves this key — and no keypad hint either, now that a
              class is a letter. */}
          <Field
            aria-label="status"
            placeholder="status — 404 or 4xx"
            className="w-field"
            value={filter.status ?? ""}
            onChange={(e) => update("status", e.target.value)}
          />
          <TonedText tone="muted" className="whitespace-nowrap">
            {flows.length} {flows.length === 1 ? "flow" : "flows"}
          </TonedText>
        </>
      }
      detail={
        selectedId == null ? undefined : (
          <FlowDetail
            key={selectedId}
            api={client}
            flow={detail}
            failed={detailFailed}
            onClose={() => setSelectedId(null)}
          />
        )
      }
    >
      {closeDialog}
      <FlowList
        flows={flows}
        selectedId={selectedId}
        onSelect={setSelectedId}
        filtered={hasFilter}
        silent={silent}
        refused={refused}
        retrying={retrying}
        onRetry={retryConnection}
      />
    </Layout>
  );
}
