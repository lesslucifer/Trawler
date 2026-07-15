import { useCallback, useEffect, useMemo, useReducer, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { createApi, defaultProxyAddr, type TrawlerApi } from "./api";
import { hostOs, splitProxyAddr, type ProxyProbe, type TrustProbe } from "./setup";
import { isDecryptedHttps } from "./flow";
import { useFlows, useFlowDetail } from "./useFlows";
import { useProxyCloseConfirm } from "./closeConfirm";
import { Layout } from "./Layout";
import { FlowList } from "./FlowList";
import { FlowDetail } from "./FlowDetail";
import { Onboarding } from "./Onboarding";
import { ProxyCloseDialog } from "./ProxyCloseDialog";
import { Field, StatusDot } from "./components";
import { dotStatus, initial, reduce, type OnbState } from "./onboardingMachine";

export default function App({ api, initialState }: { api?: TrawlerApi; initialState?: OnbState }) {
  const client = useMemo(() => api ?? createApi(), [api]);
  const { filter, flows, connections, update } = useFlows(client);
  const [state, dispatch] = useReducer(reduce, initialState, (s) => s ?? initial());
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const detail = useFlowDetail(client, selectedId);

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

  const proxyAddr = defaultProxyAddr();

  // Local probes (W6): the fast, ~90% truth for the Trust and Proxy steps — does
  // *this machine* trust the CA / route its proxy through Trawler? Runs on entry
  // and re-runs after an action so the step reflects the change immediately;
  // capture stays the authority for the remote cases. Invoke isn't available in
  // jsdom, so a failure just leaves the probe "unknown" (capture-will-confirm).
  const [trustProbe, setTrustProbe] = useState<TrustProbe>("unknown");
  const [proxyProbe, setProxyProbe] = useState<ProxyProbe>("unknown");
  const [probeNonce, setProbeNonce] = useState(0);
  const reprobe = useCallback(() => setProbeNonce((n) => n + 1), []);

  useEffect(() => {
    if (state.phase !== "onboarding") return;
    const { host, port } = splitProxyAddr(proxyAddr);
    let live = true;
    invoke<TrustProbe>("ca_trusted")
      .then((p) => live && setTrustProbe(p))
      .catch(() => live && setTrustProbe("unknown"));
    invoke<ProxyProbe>("proxy_status", { host, port })
      .then((p) => live && setProxyProbe(p))
      .catch(() => live && setProxyProbe("unknown"));
    return () => {
      live = false;
    };
  }, [state.phase, proxyAddr, probeNonce]);

  const installCa = async () => {
    const pem = await client.caCertPem();
    await invoke("install_ca", { pem });
    reprobe();
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

  if (state.phase !== "proven") {
    return (
      <Layout
        header={
          <>
            {dot}
            <h1 className="font-serif text-lg italic">Trawler</h1>
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
          observed={{ decrypted: isDecryptedHttps(flows), trafficSeen: flows.length > 0 || connections > 0 }}
          onInstallCa={installCa}
          onSetProxy={setProxy}
          onUnsetProxy={unsetProxy}
        />
      </Layout>
    );
  }

  return (
    <Layout
      header={
        <>
          {dot}
          <h1 className="mr-2 font-serif text-lg italic">Trawler</h1>
          <Field
            aria-label="filter"
            placeholder="filter — free text"
            className="min-w-32 flex-1"
            value={filter.q ?? ""}
            onChange={(e) => update("q", e.target.value)}
          />
          <Field
            aria-label="host"
            placeholder="host"
            className="w-40"
            value={filter.host ?? ""}
            onChange={(e) => update("host", e.target.value)}
          />
          <Field
            aria-label="method"
            placeholder="method"
            className="w-24"
            value={filter.method ?? ""}
            onChange={(e) => update("method", e.target.value)}
          />
          <Field
            aria-label="status"
            placeholder="status"
            inputMode="numeric"
            className="w-20"
            value={filter.status ?? ""}
            onChange={(e) => update("status", e.target.value)}
          />
          <span className="whitespace-nowrap text-ink-mute">
            {flows.length} {flows.length === 1 ? "flow" : "flows"}
          </span>
        </>
      }
      detail={
        selectedId == null ? undefined : (
          <FlowDetail
            key={selectedId}
            api={client}
            flow={detail}
            onClose={() => setSelectedId(null)}
          />
        )
      }
    >
      {closeDialog}
      <FlowList flows={flows} selectedId={selectedId} onSelect={setSelectedId} />
    </Layout>
  );
}
