import { useState } from "react";
import type { FlowView, HeaderView, TrawlerApi } from "./api";
import { statusTone, toCurl } from "./flow";
import { StatusPill } from "./components";
import { BodyViewer } from "./BodyViewer";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="border-b border-hairline px-4 py-3">
      <h2 className="mb-2 text-body uppercase tracking-label text-ink-mute">{title}</h2>
      {children}
    </section>
  );
}

function HeaderTable({ headers }: { headers: HeaderView[] }) {
  if (headers.length === 0) {
    return <p className="text-ink-mute">none</p>;
  }
  return (
    <dl
      className={
        // design-allow: grid-cols-[auto_1fr] — a two-column template shape, no size in it
        "grid grid-cols-[auto_1fr] gap-x-grid-gap-x gap-y-grid-gap-y font-mono text-body text-ink-body"
      }
    >
      {headers.map((h, i) => (
        <div key={i} className="contents">
          <dt className="whitespace-nowrap text-ink-mute">{h.name}</dt>
          <dd className="break-all">{h.value}</dd>
        </div>
      ))}
    </dl>
  );
}

export function FlowDetail({
  api,
  flow,
  onClose,
}: {
  api: TrawlerApi;
  flow: FlowView | null;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);

  if (!flow) {
    return (
      <div data-testid="detail-loading" className="p-4 font-ui text-ink-mute">
        loading…
      </div>
    );
  }

  const curl = toCurl(flow);
  const copyCurl = () => {
    navigator.clipboard?.writeText(curl).then(
      () => setCopied(true),
      () => {},
    );
  };

  return (
    <div data-testid="detail" className="flex h-full flex-col font-ui text-ink">
      <header className="flex items-center gap-2 border-b border-hairline px-4 py-2">
        <span className="font-mono text-ink-body">{flow.method}</span>
        <StatusPill tone={statusTone(flow.status)}>
          {flow.status == null ? "…" : flow.status}
        </StatusPill>
        <span className="ml-auto whitespace-nowrap text-body text-ink-mute">
          {flow.duration_ms == null ? "" : `${flow.duration_ms} ms`}
        </span>
        <button
          type="button"
          aria-label="close"
          onClick={onClose}
          className="text-ink-mute hover:text-ink"
        >
          ×
        </button>
      </header>

      <div className="min-h-0 flex-1 overflow-auto">
        <Section title="Request">
          <p className="break-all font-mono text-body text-ink-body">{flow.uri}</p>
          <p className="mt-chrome-y text-body text-ink-mute">
            {flow.request_http_version}
            {flow.response_http_version ? ` → ${flow.response_http_version}` : ""}
          </p>
        </Section>

        <Section title="Copy as curl">
          <div className="flex items-start gap-2">
            <pre
              data-testid="curl"
              className="min-w-0 flex-1 overflow-x-auto whitespace-pre-wrap break-all rounded-button border border-hairline bg-canvas px-2 py-chrome-y font-mono text-body text-ink-body"
            >
              {curl}
            </pre>
            <button
              type="button"
              onClick={copyCurl}
              className="shrink-0 rounded-button border border-hairline px-2 py-chrome-y text-body text-ink-mute hover:text-ink"
            >
              {copied ? "copied" : "copy"}
            </button>
          </div>
        </Section>

        <Section title="Request headers">
          <HeaderTable headers={flow.request_headers} />
        </Section>

        <Section title="Response headers">
          <HeaderTable headers={flow.response_headers} />
        </Section>

        <Section title="Body">
          <BodyViewer api={api} flow={flow} />
        </Section>
      </div>
    </div>
  );
}
