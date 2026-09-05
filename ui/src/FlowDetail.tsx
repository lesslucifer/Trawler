import { useCallback, useEffect, useRef, useState } from "react";
import type { FlowView, HeaderView, Leg, TrawlerApi } from "./api";
import { toCurl } from "./flow";
import {
  CopyButton,
  CopyText,
  DurationText,
  IconButton,
  KeyValueRow,
  MethodLabel,
  Notice,
  Section,
  SegmentedControl,
  StatusPill,
} from "./components/index";
import { BodyViewer } from "./BodyViewer";

const LEGS = [
  { value: "request", label: "request" },
  { value: "response", label: "response" },
];

function LegHeaders({ headers }: { headers: HeaderView[] }) {
  if (headers.length === 0) {
    return <Notice>none — this leg sent no headers</Notice>;
  }
  return (
    <dl
      data-wire="collection"
      data-order="wire"
      className={
        // design-allow: grid-cols-[auto_1fr] — a two-column template shape, no size in it
        "grid grid-cols-[auto_1fr] gap-x-grid-gap-x gap-y-grid-gap-y"
      }
    >
      {headers.map((h, i) => (
        <KeyValueRow key={i} name={h.name} value={h.value} />
      ))}
    </dl>
  );
}

function versionsLine(flow: FlowView): string {
  return flow.response_http_version
    ? `${flow.request_http_version} → ${flow.response_http_version}`
    : flow.request_http_version;
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
  const [leg, setLeg] = useState<Leg>("request");

  // The list keeps focus when Enter opens the panel, so the origin is read once
  // at mount — before anything inside the panel can take focus — and Esc is
  // heard at the document, not on the panel's own subtree.
  const origin = useRef<Element | null>(null);
  useEffect(() => {
    origin.current = document.activeElement;
  }, []);

  const close = useCallback(() => {
    onClose();
    const target = origin.current;
    if (target instanceof HTMLElement && target.isConnected) target.focus();
  }, [onClose]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) close();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [close]);

  if (!flow) {
    return (
      <div data-testid="detail-loading" className="px-4 font-ui">
        <Notice>loading this flow…</Notice>
      </div>
    );
  }

  const headers = leg === "request" ? flow.request_headers : flow.response_headers;

  return (
    <div data-testid="detail" className="flex h-full flex-col font-ui text-ink">
      <header className="flex shrink-0 items-center gap-2 border-b border-hairline px-4 py-2">
        <MethodLabel method={flow.method} />
        <StatusPill status={flow.status} />
        <SegmentedControl
          options={LEGS}
          value={leg}
          onChange={(value) => setLeg(value as Leg)}
          ariaLabel="leg"
        />
        <span className="ml-auto flex items-center gap-2">
          <DurationText value={flow.duration_ms} />
          <IconButton glyph="close" onClick={close} />
        </span>
      </header>

      <div className="min-h-0 flex-1 overflow-auto">
        <div data-testid="flow-line" className="border-b border-hairline px-4 py-3">
          <CopyText value={flow.uri} label="uri" />
          <div className="mt-chrome-y flex items-center gap-2">
            <span data-wire="protocol-version" className="font-mono text-body text-ink-mute">
              {versionsLine(flow)}
            </span>
            <CopyButton className="ml-auto" label="copy as curl" getPayload={() => toCurl(flow)} />
          </div>
        </div>

        <Section title="Headers">
          <LegHeaders headers={headers} />
        </Section>

        <Section title="Body">
          <BodyViewer api={api} flow={flow} leg={leg} />
        </Section>
      </div>
    </div>
  );
}
