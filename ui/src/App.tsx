import { useEffect, useMemo, useReducer } from "react";
import { invoke } from "@tauri-apps/api/core";
import { createApi, type TrawlerApi } from "./api";
import { hostOs } from "./setup";
import { useFlows } from "./useFlows";
import { Layout } from "./Layout";
import { FlowList } from "./FlowList";
import { Onboarding } from "./Onboarding";
import { Field, StatusDot } from "./components";
import { dotStatus, initial, reduce, type OnbState } from "./onboardingMachine";

export default function App({ api, initialState }: { api?: TrawlerApi; initialState?: OnbState }) {
  const client = useMemo(() => api ?? createApi(), [api]);
  const { filter, flows, update } = useFlows(client);
  const [state, dispatch] = useReducer(reduce, initialState, (s) => s ?? initial());

  // W1 placeholder for W4's launch self-test: nothing detects live capture yet,
  // so a fresh launch falls straight through to onboarding.
  useEffect(() => {
    if (state.phase === "checking") dispatch({ t: "not-detected" });
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const installCa = async () => {
    const pem = await client.caCertPem();
    await invoke("install_ca", { pem });
  };

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
        <Onboarding state={state} dispatch={dispatch} os={hostOs()} onInstallCa={installCa} />
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
    >
      <FlowList flows={flows} />
    </Layout>
  );
}
