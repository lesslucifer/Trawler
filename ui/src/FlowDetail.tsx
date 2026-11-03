import { useCallback, useEffect, useRef, useState } from "react";
import type { FlowView, HeaderView, Leg, TrawlerApi } from "./api";
import { toCurl } from "./flow";
import {
  Button,
  CopyButton,
  CopyText,
  DurationText,
  IconButton,
  KeyValueRow,
  MethodLabel,
  Notice,
  Section,
  SegmentedControl,
  Splitter,
  StatusPill,
} from "./components/index";
import { BodyViewer } from "./BodyViewer";
import { HEADERS_SHARE } from "./layoutBounds";
import type { LayoutControls } from "./useLayout";

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
        <KeyValueRow key={i} name={h.name} value={h.value} collapsible />
      ))}
    </dl>
  );
}

function useHeight(): [(el: HTMLElement | null) => void, number] {
  const [height, setHeight] = useState(0);
  const observer = useRef<ResizeObserver | null>(null);
  const measure = useCallback((el: HTMLElement | null) => {
    observer.current?.disconnect();
    observer.current = null;
    if (!el) return;
    setHeight(el.getBoundingClientRect().height);
    if (typeof ResizeObserver === "undefined") return;
    observer.current = new ResizeObserver(([entry]) => setHeight(entry.contentRect.height));
    observer.current.observe(el);
  }, []);
  return [measure, height];
}

function versionsLine(flow: FlowView): string {
  return flow.response_http_version
    ? `${flow.request_http_version} → ${flow.response_http_version}`
    : flow.request_http_version;
}

export function FlowDetail({
  api,
  flow,
  failed = false,
  onClose,
  onReplay,
  split,
}: {
  api: TrawlerApi;
  flow: FlowView | null;
  failed?: boolean;
  onClose: () => void;
  onReplay?: () => void;
  split: LayoutControls;
}) {
  const [leg, setLeg] = useState<Leg>("request");
  const [measure, height] = useHeight();
  const [measureHeaders, headersHeight] = useHeight();

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
    // "loading this flow…" is the one sentence that is false once the fetch has
    // come back with nothing, so the pane says what it has instead. It names no
    // cause and raises no alarm: the causes it would have to tell apart are the
    // shell's (a core not answering, stated in the band) or a later run's (a
    // flow the ring dropped, which "no longer captured" is reserved for), and
    // one warn on screen per failure is what keeps the band's the only one.
    return failed ? (
      <div data-testid="detail-failed" className="px-4 font-ui">
        <Notice>could not load this flow</Notice>
      </div>
    ) : (
      <div data-testid="detail-loading" className="px-4 font-ui">
        <Notice>loading this flow…</Notice>
      </div>
    );
  }

  const headers = leg === "request" ? flow.request_headers : flow.response_headers;
  const ceiling = split.layout.headersShare;
  const headersAt = height > 0 ? Math.min(ceiling, headersHeight / height) : ceiling;

  return (
    <div ref={measure} data-testid="detail" className="flex h-full flex-col font-ui text-ink">
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

      <div data-testid="flow-line" className="shrink-0 border-b border-hairline px-4 py-3">
        <CopyText value={flow.uri} label="uri" />
        <div className="mt-chrome-y flex items-center gap-2">
          <span data-wire="protocol-version" className="font-mono text-body text-ink-mute">
            {versionsLine(flow)}
          </span>
          {/* Both send this request again: one elsewhere, one here. */}
          <span className="ml-auto flex items-center gap-2">
            {onReplay && <Button onClick={onReplay}>replay</Button>}
            <CopyButton label="copy as curl" getPayload={() => toCurl(flow)} />
          </span>
        </div>
      </div>

      {/* The body is the zone a reader came for, so it takes whatever height the
          headers leave: the headers zone is capped at the user's share of the
          panel and scrolls inside itself, the body fills the rest and scrolls
          inside itself, and the panel as a whole never scrolls. The share is a
          ceiling, so a short leg keeps its own height and the splitter sits
          under its last row — which is where a drag or a step starts from. */}
      <div
        ref={measureHeaders}
        data-testid="headers-zone"
        className="flex min-h-0 flex-col"
        style={{ maxHeight: `${ceiling * 100}%` }}
      >
        <Section title={`Headers · ${headers.length}`} className="flex min-h-0 flex-col">
          <div data-testid="headers-scroll" className="relative min-h-0 overflow-auto">
            <LegHeaders headers={headers} />
          </div>
        </Section>
      </div>
      <div className="-mt-px">
        <Splitter
          axis="y"
          dir={1}
          perPx={1 / Math.max(height, 1)}
          value={headersAt}
          min={HEADERS_SHARE.min}
          max={HEADERS_SHARE.max}
          onChange={split.setHeadersShare}
          onCommit={split.commit}
          onReset={split.resetHeadersShare}
          label="headers height"
          valueText={`${Math.round(headersAt * 100)} % of the pane`}
        />
      </div>

      <Section title="Body" className="flex min-h-0 flex-1 flex-col">
        <BodyViewer api={api} flow={flow} leg={leg} />
      </Section>
    </div>
  );
}
