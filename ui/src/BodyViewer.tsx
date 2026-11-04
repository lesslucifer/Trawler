import { memo, useEffect, useState } from "react";
import type { BodyFetch, BodyView, FlowView, Leg, TrawlerApi } from "./api";
import {
  bodyKind,
  bodyNotice,
  bodyText,
  decodeBody,
  hexDump,
  rawKind,
  saveBody,
  saveFilename,
  type BodyKind,
} from "./body";
import {
  CopyButton,
  FormRenderer,
  HexRenderer,
  IconButton,
  ImageRenderer,
  JsonRenderer,
  Notice,
  SegmentedControl,
  TextRenderer,
  XmlRenderer,
} from "./components/index";

const MODES = [
  { value: "pretty", label: "pretty" },
  { value: "raw", label: "raw" },
];

function Dispatch({ fetched, kind }: { fetched: BodyFetch; kind: BodyKind }) {
  switch (kind) {
    case "json":
      return <JsonRenderer text={bodyText(fetched.bytes)} />;
    case "xml":
      return <XmlRenderer text={bodyText(fetched.bytes)} />;
    case "form":
      return <FormRenderer text={bodyText(fetched.bytes)} />;
    case "image":
      return <ImageRenderer bytes={fetched.bytes} contentType={fetched.contentType} />;
    case "hex":
      return <HexRenderer bytes={fetched.bytes} />;
    case "text": {
      const decoded = decodeBody(fetched.bytes);
      return (
        <TextRenderer
          text={decoded.text}
          size={decoded.whole ? fetched.bytes.length : undefined}
        />
      );
    }
  }
}

// Memoized because its parent re-renders on every move of a splitter drag, and
// a re-render here decodes the whole body again.
export const BodyViewer = memo(function BodyViewer({
  api,
  flow,
  leg,
  pretty,
  onPretty,
}: {
  api: TrawlerApi;
  flow: FlowView;
  leg: Leg;
  pretty: boolean;
  onPretty: (pretty: boolean) => void;
}) {
  const [fetched, setFetched] = useState<BodyFetch | "error" | null>(null);

  const body: BodyView | null = leg === "request" ? flow.request_body : flow.response_body;
  const notice = bodyNotice(body);
  const loaded = fetched !== null && fetched !== "error" ? fetched : null;
  const kind: BodyKind | null =
    loaded && (pretty ? bodyKind(loaded.contentType) : rawKind(loaded.bytes));

  useEffect(() => {
    setFetched(null);
    if (body?.state !== "captured") return;
    let cancelled = false;
    api.getBody(flow.id, leg, pretty).then(
      (result) => {
        if (!cancelled) setFetched(result);
      },
      () => {
        if (!cancelled) setFetched("error");
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api, flow.id, leg, pretty, body?.state]);

  // Copy delivers what the pane shows: the hex dump for hex-rendered bodies,
  // the body text otherwise. An image has no text form — ImageRenderer's @avoid
  // is why there is no copy button for that kind at all.
  const shownText = () =>
    loaded == null ? "" : kind === "hex" ? hexDump(loaded.bytes) : bodyText(loaded.bytes);

  const save = () => {
    if (!loaded) return;
    void saveBody(loaded.bytes, saveFilename(flow.uri, leg, loaded.contentType));
  };

  return (
    <div data-testid="body-viewer" data-leg={leg} className="flex min-h-0 flex-1 flex-col gap-2">
      <div className="flex items-center gap-chrome-gap">
        <SegmentedControl
          options={MODES}
          value={pretty ? "pretty" : "raw"}
          onChange={(value) => onPretty(value === "pretty")}
          ariaLabel={`${leg} body mode`}
        />
        {loaded && (
          <span className="ml-auto flex items-center gap-chrome-gap">
            {kind !== "image" && (
              <CopyButton label="copy body" getPayload={shownText} />
            )}
            <IconButton glyph="save" onClick={save} label="save" />
          </span>
        )}
      </div>

      {notice != null ? (
        <Notice>{notice}</Notice>
      ) : fetched === "error" ? (
        <Notice tone="warn">failed to load body</Notice>
      ) : !loaded || kind == null ? (
        <Notice>loading…</Notice>
      ) : (
        // The cut marker rides in the member's own output, so B's scroll box is
        // what would carry it out of sight — the one line whose job is to be
        // seen. B fills the height its host gives and scrolls inside it, which
        // keeps the marker at its foot. `relative` is load-bearing: WebKit lets
        // a static scroll container's overflow leak into the document's
        // scrollable area when its content is a display:contents grid.
        <div data-testid="body-pane" className="relative min-h-0 flex-1 overflow-auto">
          <Dispatch fetched={loaded} kind={kind} />
        </div>
      )}
    </div>
  );
});
