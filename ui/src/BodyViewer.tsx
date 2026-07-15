import { useEffect, useState } from "react";
import type { BodyFetch, BodyView, FlowView, Leg, TrawlerApi } from "./api";
import {
  bodyKind,
  bodyNotice,
  bodyText,
  formEntries,
  hexDump,
  rawKind,
  saveBody,
  saveFilename,
  TREE_MAX,
  type BodyKind,
} from "./body";

function JsonNode({ name, value }: { name: string; value: unknown }) {
  if (value !== null && typeof value === "object") {
    const entries = Object.entries(value);
    const shape = Array.isArray(value) ? `[${entries.length}]` : `{${entries.length}}`;
    return (
      <details open>
        <summary className="cursor-pointer text-ink-mute">
          {name} <span className="text-ink-body">{shape}</span>
        </summary>
        <div className="border-l border-hairline pl-4">
          {entries.map(([key, child]) => (
            <JsonNode key={key} name={key} value={child} />
          ))}
        </div>
      </details>
    );
  }
  return (
    <div>
      <span className="text-ink-mute">{name}: </span>
      <span className="break-all">{JSON.stringify(value)}</span>
    </div>
  );
}

function JsonTree({ text }: { text: string }) {
  if (text.length > TREE_MAX) return <TextBody text={text} />;
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return <TextBody text={text} />;
  }
  return (
    <div data-testid="body-json" className="font-mono text-xs text-ink-body">
      <JsonNode name="$" value={parsed} />
    </div>
  );
}

function TextBody({ text }: { text: string }) {
  return (
    <pre
      data-testid="body-text"
      className="overflow-x-auto whitespace-pre-wrap break-all font-mono text-xs text-ink-body"
    >
      {text}
    </pre>
  );
}

function FormBody({ text }: { text: string }) {
  return (
    <dl
      data-testid="body-form"
      className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 font-mono text-xs text-ink-body"
    >
      {formEntries(text).map(([key, value], i) => (
        <div key={i} className="contents">
          <dt className="whitespace-nowrap text-ink-mute">{key}</dt>
          <dd className="break-all">{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function ImageBody({ bytes, contentType }: { bytes: Uint8Array; contentType: string | null }) {
  const [url, setUrl] = useState<string>();
  useEffect(() => {
    const objectUrl = URL.createObjectURL(new Blob([bytes], { type: contentType ?? "" }));
    setUrl(objectUrl);
    return () => URL.revokeObjectURL(objectUrl);
  }, [bytes, contentType]);
  if (!url) return null;
  return (
    <img
      data-testid="body-image"
      src={url}
      alt="body"
      className="max-w-full border border-hairline"
    />
  );
}

function Rendered({ fetched, kind }: { fetched: BodyFetch; kind: BodyKind }) {
  switch (kind) {
    case "json":
      return <JsonTree text={bodyText(fetched.bytes)} />;
    case "image":
      return <ImageBody bytes={fetched.bytes} contentType={fetched.contentType} />;
    case "form":
      return <FormBody text={bodyText(fetched.bytes)} />;
    case "text":
      return <TextBody text={bodyText(fetched.bytes)} />;
    case "hex":
      return <TextBody text={hexDump(fetched.bytes)} />;
  }
}

function ModeButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: string;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={`rounded-button border border-hairline px-2 py-0.5 text-xs ${
        active ? "bg-canvas-soft text-ink" : "text-ink-mute hover:text-ink"
      }`}
    >
      {children}
    </button>
  );
}

export function BodyViewer({ api, flow }: { api: TrawlerApi; flow: FlowView }) {
  const [leg, setLeg] = useState<Leg>(flow.response_body ? "response" : "request");
  const [decode, setDecode] = useState(true);
  const [fetched, setFetched] = useState<BodyFetch | "error" | null>(null);
  const [copied, setCopied] = useState(false);

  const body: BodyView | null = leg === "request" ? flow.request_body : flow.response_body;
  const notice = bodyNotice(body);
  const loaded = fetched !== null && fetched !== "error" ? fetched : null;
  const kind: BodyKind | null =
    loaded && (decode ? bodyKind(loaded.contentType) : rawKind(loaded.bytes));

  useEffect(() => {
    setFetched(null);
    setCopied(false);
    if (body?.state !== "captured") return;
    let cancelled = false;
    api.getBody(flow.id, leg, decode).then(
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
  }, [api, flow.id, leg, decode, body?.state]);

  // Copy delivers what the viewer shows: the hex dump for hex-rendered bodies,
  // the body text otherwise. Images have no text representation — no copy.
  const copyBody = () => {
    if (!loaded) return;
    const shown = kind === "hex" ? hexDump(loaded.bytes) : bodyText(loaded.bytes);
    navigator.clipboard?.writeText(shown).then(
      () => setCopied(true),
      () => {},
    );
  };

  const save = () => {
    if (!loaded) return;
    void saveBody(loaded.bytes, saveFilename(flow.id, leg, loaded.contentType));
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-1">
        <ModeButton active={leg === "request"} onClick={() => setLeg("request")}>
          request
        </ModeButton>
        <ModeButton active={leg === "response"} onClick={() => setLeg("response")}>
          response
        </ModeButton>
        <span className="mx-1 text-ink-mute">·</span>
        <ModeButton active={decode} onClick={() => setDecode(true)}>
          decoded
        </ModeButton>
        <ModeButton active={!decode} onClick={() => setDecode(false)}>
          raw
        </ModeButton>
        {loaded && (
          <span className="ml-auto flex gap-1">
            {kind !== "image" && (
              <ModeButton active={false} onClick={copyBody}>
                {copied ? "copied" : "copy body"}
              </ModeButton>
            )}
            <ModeButton active={false} onClick={save}>
              save
            </ModeButton>
          </span>
        )}
      </div>

      {notice != null ? (
        <p data-testid="body-notice" className="text-xs text-ink-mute">
          {notice}
        </p>
      ) : fetched === "error" ? (
        <p data-testid="body-notice" className="text-xs text-warn">
          failed to load body
        </p>
      ) : !loaded || kind == null ? (
        <p className="text-xs text-ink-mute">loading…</p>
      ) : (
        <Rendered fetched={loaded} kind={kind} />
      )}
    </div>
  );
}
