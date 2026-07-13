import { useMemo, useState } from "react";
import { createApi, type TrawlerApi } from "./api";
import { useFlows, useFlowDetail } from "./useFlows";
import { Layout } from "./Layout";
import { FlowList } from "./FlowList";
import { FlowDetail } from "./FlowDetail";
import { Field, LiveDot } from "./components";

export default function App({ api }: { api?: TrawlerApi }) {
  const client = useMemo(() => api ?? createApi(), [api]);
  const { filter, flows, live, update } = useFlows(client);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const detail = useFlowDetail(client, selectedId);

  return (
    <Layout
      header={
        <>
          <LiveDot live={live} />
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
      <FlowList flows={flows} selectedId={selectedId} onSelect={setSelectedId} />
    </Layout>
  );
}
