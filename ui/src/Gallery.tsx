import { useState } from "react";
import {
  Button,
  CopyButton,
  CopyText,
  CutMarker,
  DurationText,
  FormRenderer,
  HexRenderer,
  IconButton,
  ImageRenderer,
  JsonRenderer,
  KeyValueRow,
  MethodLabel,
  Notice,
  Section,
  SegmentedControl,
  Select,
  StatusLine,
  StatusPill,
  StepList,
  TextRenderer,
  TimestampText,
  TonedText,
  XmlRenderer,
} from "./components/index";
import { proxyStatus, trustStatus } from "./setup";

// ponytail: throwaway verification screen for E-DFLOW.W5 step 4 — every T1
// component in every state its design file names, so a human can eyeball what
// the unit tests assert. Delete this file and its two lines in main.tsx once the
// collection is signed off; nothing in the app imports it.

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-start gap-4 border-b border-hairline/40 py-2 last:border-0">
      <span className="w-52 shrink-0 pt-0.5 font-mono text-label uppercase tracking-wide text-ink-mute">
        {label}
      </span>
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-3">{children}</div>
    </div>
  );
}

const TARGETS = [
  { id: "ios", label: "iOS Simulator" },
  { id: "android", label: "Android Emulator" },
  { id: "chrome", label: "Chrome" },
  { id: "firefox", label: "Firefox" },
  { id: "system", label: "System proxy" },
  { id: "curl", label: "curl / CLI" },
  { id: "postman", label: "Postman" },
  { id: "other", label: "Something else" },
];

const URI = "https://api.example.com/v2/orders/8f3a91c2/items?expand=pricing&currency=USD";

const JSON_BODY =
  '{"id":"ord_8f3a91c2","status":"pending","total":49.99,"paid":false,"items":[{"sku":"p1","qty":2},{"sku":"p7","qty":1}],"tags":["rush","gift"]}';

const XML_BODY =
  '<order id="8f3a91c2">\n  <status>pending</status>\n  <items>\n    <item sku="p1">SKU-001</item>\n  </items>\n</order>';

// A 16×16 RGBA PNG, half its squares fully transparent — so the checkerboard
// behind ImageRenderer's preview is actually visible.
const PNG_BASE64 =
  "iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAYAAAAf8/9hAAAAI0lEQVR42mPwOdz1HxkzoAFC8gzDwIBh6CWSDRhNB6PpAIgBE45MEJeL5NUAAAAASUVORK5CYII=";

const PNG_BYTES = Uint8Array.from(atob(PNG_BASE64), (c) => c.charCodeAt(0));

export function Gallery() {
  const [leg, setLeg] = useState("request");
  const [view, setView] = useState("raw");
  const [target, setTarget] = useState<string | undefined>(undefined);
  const [chosen, setChosen] = useState<string | undefined>("chrome");
  const [inFlight, setInFlight] = useState(false);
  const [fired, setFired] = useState<string | null>(null);

  const act = (what: string) => () => {
    setFired(what);
    setTimeout(() => setFired(null), 1600);
  };

  return (
    <div className="h-full overflow-y-auto bg-canvas font-ui text-ink">
      <header className="sticky top-0 z-10 border-b border-hairline bg-canvas px-4 py-3">
        <h1 className="font-serif text-lg italic">Component gallery</h1>
        <p className="mt-1 text-chrome text-ink-mute">
          E-DFLOW.W5 step 4 — every T1 component, every state. Throwaway screen.
        </p>
        {fired && (
          <p className="mt-1 font-mono text-chrome text-ok">handler fired → {fired}</p>
        )}
      </header>

      <Section title="Button — the only <button> element">
        <Row label="rest / hover / focus">
          <Button onClick={act("retry")}>retry</Button>
          <Button onClick={act("close")}>close</Button>
          <span className="text-chrome text-ink-mute">hover and Tab to these</span>
        </Row>
        <Row label="disabled = in-flight (D4)">
          <Button onClick={act("never")} disabled>
            trust the certificate
          </Button>
          <span className="font-ui text-body text-ink-mute">⟳ waiting for the system prompt…</span>
        </Row>
        <Row label="optional leading glyph">
          <Button onClick={act("retry+glyph")} glyph="↻">
            retry
          </Button>
        </Row>
        <Row label="axis: tone">
          <Button onClick={act("body")}>tone=body</Button>
          <Button onClick={act("mute")} tone="mute">
            tone=mute
          </Button>
        </Row>
        <Row label="axis: frame">
          <Button onClick={act("border")}>frame=border</Button>
          <Button onClick={act("bare")} frame="bare">
            frame=bare
          </Button>
        </Row>
        <Row label="axis: shape (square)">
          <Button onClick={act("sq24")} shape="square" label="24px square">
            ⧉
          </Button>
          <Button onClick={act("sq28")} shape="square" size={28} label="28px square">
            ×
          </Button>
          <span className="text-chrome text-ink-mute">24 default · 28 · floor clamps below 24</span>
        </Row>
        <Row label="in a warn zone (D2)">
          <Notice tone="warn" action={<Button onClick={act("retry in notice")}>retry</Button>}>
            could not load this flow — 500
          </Notice>
        </Row>
      </Section>

      <Section title="CopyButton — labelled + icon forms">
        <Row label="labelled (click me)">
          <CopyButton label="copy curl" value={`curl '${URI}'`} />
          <CopyButton label="copy body" value='{"ok":true}' />
          <span className="text-chrome text-ink-mute">→ "copied" ~2 s, no width shift</span>
        </Row>
        <Row label="icon form D2b — 24px">
          <CopyButton label="copy body" value='{"ok":true}' iconOnly />
          <span className="text-chrome text-ink-mute">⧉ → ✓ · tooltip + aria-label</span>
        </Row>
        <Row label="with leading icon">
          <CopyButton label="curl" value="curl x" icon="⧉" />
        </Row>
        <Row label="failure → 'failed' warn">
          <CopyButton label="copy (will fail)" getPayload={() => { throw new Error("nope"); }} />
          <span className="text-chrome text-ink-mute">throws in getPayload</span>
        </Row>
        <Row label="disabled">
          <CopyButton label="copy body" value="x" disabled />
        </Row>
      </Section>

      <Section title="IconButton — bare glyph">
        <Row label="registry glyphs">
          <IconButton glyph="close" onClick={act("close")} />
          <IconButton glyph="save" onClick={act("save")} />
          <span className="text-chrome text-ink-mute">× and ↓ · 28px · hover for the wash</span>
        </Row>
        <Row label="custom glyph + label">
          <IconButton label="collapse panel" onClick={act("collapse")}>
            ⌄
          </IconButton>
        </Row>
        <Row label="size / disabled">
          <IconButton glyph="close" onClick={act("s24")} size={24} />
          <IconButton glyph="close" onClick={act("s32")} size={32} />
          <IconButton glyph="close" onClick={act("never")} disabled />
        </Row>
      </Section>

      <Section title="TonedText — the tone channel">
        <Row label="three tones">
          <TonedText tone="ok">✓ trusted</TonedText>
          <TonedText tone="warn">⚠ not trusted</TonedText>
          <TonedText tone="muted">… checking</TonedText>
        </Row>
        <Row label="inherits size from its host">
          <span className="text-label">
            <TonedText tone="ok">10px</TonedText>
          </span>
          <span className="text-value">
            <TonedText tone="ok">12.5px</TonedText>
          </span>
          <span className="text-lg">
            <TonedText tone="ok">large</TonedText>
          </span>
        </Row>
      </Section>

      <Section title="StatusPill — HTTP status → tone (T2)">
        <Row label="ok · 2xx">
          <StatusPill status={200} />
          <StatusPill status={201} />
          <StatusPill status={204} />
        </Row>
        <Row label="warn · 4xx / 5xx">
          <StatusPill status={404} />
          <StatusPill status={418} />
          <StatusPill status={500} />
          <StatusPill status={503} />
        </Row>
        <Row label="muted · 1xx / 3xx / unknown">
          <StatusPill status={101} />
          <StatusPill status={301} />
          <StatusPill status={799} />
        </Row>
        <Row label="pending · null">
          <StatusPill status={null} />
          <span className="text-chrome text-ink-mute">hover any pill for its reason phrase</span>
        </Row>
      </Section>

      <Section title="CopyText — the displayed value you grab whole">
        <Row label="click to copy">
          <CopyText value={URI} label="request URI" />
        </Row>
        <Row label="selection guard">
          <span className="text-chrome text-ink-mute">
            drag-select part of the URI above and release — no copy fires, ⌘C takes your selection
          </span>
        </Row>
        <Row label="wraps, never truncates">
          <div className="w-64 border border-hairline/50 p-2">
            <CopyText value={URI} label="request URI" />
          </div>
        </Row>
      </Section>

      <Section title="KeyValueRow — one wire pair">
        <dl className="grid grid-cols-[auto_1fr] gap-x-grid-gap-x gap-y-grid-gap-y">
          <KeyValueRow name="content-type" value="application/json; charset=utf-8" />
          <KeyValueRow name="cache-control" value="no-store, max-age=0" />
          <KeyValueRow name="set-cookie" value={"session=" + "a1b2c3d4".repeat(12)} />
        </dl>
        <p className="mt-2 text-chrome text-ink-mute">
          the value cell is a CopyText — click one; the name is inert
        </p>
      </Section>

      <Section title="DurationText — the format ladder">
        <Row label="integer ms below 1s">
          <DurationText value={0} /> · <DurationText value={1} /> · <DurationText value={327} /> ·{" "}
          <DurationText value={999} />
        </Row>
        <Row label="one decimal above">
          <DurationText value={1000} /> · <DurationText value={1842} /> ·{" "}
          <DurationText value={59999} /> · <DurationText value={60000} /> ·{" "}
          <DurationText value={5400000} /> · <DurationText value={129600000} />
        </Row>
        <Row label="input unit normalizes">
          <DurationText value={1.5} unit="s" /> · <DurationText value={90} unit="s" /> ·{" "}
          <DurationText value={36} unit="h" />
        </Row>
        <Row label="pending holds the column">
          <span className="border border-hairline/50">
            <DurationText value={null} />
          </span>
          <span className="text-chrome text-ink-mute">invisible "000 ms" — never a ticker</span>
        </Row>
      </Section>

      <Section title="TimestampText — local wall-clock">
        <Row label="same day">
          <TimestampText timestamp={new Date()} />
        </Row>
        <Row label="cross day / cross year">
          <TimestampText timestamp={Date.now() - 86400000} />
          <TimestampText timestamp="2025-12-31T08:00:00.000Z" />
        </Row>
        <Row label="absent / unparseable">
          <TimestampText timestamp={null} />
          <TimestampText timestamp="not a date" />
        </Row>
        <Row label="custom format">
          <TimestampText timestamp={new Date()} format="HH:mm:ss.SSS" />
          <TimestampText timestamp={new Date()} format="hh:mm:ss A" />
        </Row>
        <Row label="relative (10s tick)">
          <TimestampText timestamp={Date.now() - 180000} format="relative" />
        </Row>
      </Section>

      <Section title="SegmentedControl">
        <Row label="two options">
          <SegmentedControl
            options={[
              { value: "request", label: "request" },
              { value: "response", label: "response" },
            ]}
            value={leg}
            onChange={setLeg}
            ariaLabel="leg"
          />
          <span className="font-mono text-chrome text-ink-mute">value = {leg}</span>
        </Row>
        <Row label="three options + keyboard">
          <SegmentedControl
            options={[
              { value: "raw", label: "raw" },
              { value: "decoded", label: "decoded" },
              { value: "hex", label: "hex" },
            ]}
            value={view}
            onChange={setView}
            ariaLabel="body view"
          />
          <span className="text-chrome text-ink-mute">
            Tab in, then ←/→ move focus without selecting; Space/Enter commits
          </span>
        </Row>
        <Row label="disabled group">
          <SegmentedControl
            options={[
              { value: "raw", label: "raw" },
              { value: "decoded", label: "decoded" },
            ]}
            value="raw"
            onChange={() => {}}
            ariaLabel="disabled example"
            disabled
          />
        </Row>
      </Section>

      <Section title="Notice — three tones">
        <Row label="mute (absence)">
          <Notice>no headers on this leg</Notice>
        </Row>
        <Row label="mute + detail">
          <Notice detail="this flow was evicted from the capture ring">
            body not available
          </Notice>
        </Row>
        <Row label="warn + ⚠">
          <Notice tone="warn">could not load this flow — 500</Notice>
        </Row>
        <Row label="warn + detail + action">
          <Notice
            tone="warn"
            detail="trawler stopped responding on 127.0.0.1:7421"
            action={
              <>
                <Button onClick={act("retry")}>retry</Button>
                <Button onClick={act("close")}>close</Button>
              </>
            }
          >
            could not reach the proxy
          </Notice>
        </Row>
        <Row label="ok + ✓ (new in W5)">
          <Notice tone="ok">trawler is capturing</Notice>
        </Row>
        <Row label="ok + detail">
          <Notice tone="ok" detail="12 flows so far, 3 decrypted">
            trawler is capturing
          </Notice>
        </Row>
      </Section>

      <Section title="StepList — ordered manual instructions">
        <StepList
          steps={[
            "open Settings › General › About",
            "scroll to Certificate Trust Settings",
            <>
              <span>run this, then re-check:</span>
              <CopyText value="trawler ca install --system" label="install command" />
            </>,
            "toggle Trawler CA on",
          ]}
        />
        <p className="mt-2 text-chrome text-ink-mute">
          step 3 holds a CopyText — the slot takes any node; long commands wrap
        </p>
      </Section>

      <Section title="Select — the one dropdown">
        <Row label="nothing selected">
          <Select
            options={TARGETS}
            value={target}
            onChange={setTarget}
            label="trust target"
          />
          <span className="font-mono text-chrome text-ink-mute">
            value = {target ?? "undefined"}
          </span>
        </Row>
        <Row label="with a selection + tick">
          <Select options={TARGETS} value={chosen} onChange={setChosen} label="picked target" />
          <span className="text-chrome text-ink-mute">
            type to filter · ↑/↓ move · Enter commits · Esc closes unchanged
          </span>
        </Row>
        <Row label="loading (outranks empty)">
          <Select options={[]} value={undefined} onChange={() => {}} isLoading label="loading" />
        </Row>
        <Row label="loaded but empty">
          <Select options={[]} value={undefined} onChange={() => {}} label="empty" />
        </Row>
        <Row label="searchable=false">
          <Select
            options={TARGETS}
            value={undefined}
            onChange={() => {}}
            searchable={false}
            label="no search row"
          />
          <span className="text-chrome text-ink-mute">typing still filters and reveals the row</span>
        </Row>
        <Row label="option elements">
          <Select
            options={TARGETS.slice(0, 4).map((t) => ({
              ...t,
              element: (
                <span className="flex items-center gap-2">
                  <TonedText tone="ok">●</TonedText>
                  {t.label}
                </span>
              ),
            }))}
            value={undefined}
            onChange={() => {}}
            label="rich options"
          />
        </Row>
        <Row label="disabled">
          <Select options={TARGETS} value="chrome" onChange={() => {}} disabled label="disabled" />
        </Row>
      </Section>

      <Section title="Section — variant=band (F3)" variant="band">
        <Section title="request">
          <p className="text-body text-ink-body">an h3 member inside the band</p>
        </Section>
        <Section title="response">
          <p className="text-body text-ink-body">a second h3 member — padding never doubles</p>
        </Section>
      </Section>

      <Section title="The in-flight contract (Button D4), live">
        <Row label="press it">
          <Button
            onClick={() => {
              setInFlight(true);
              setTimeout(() => setInFlight(false), 2500);
            }}
            disabled={inFlight}
          >
            trust the certificate
          </Button>
          {inFlight ? (
            <span className="font-ui text-body text-ink-mute">
              ⟳ waiting for the system prompt…
            </span>
          ) : (
            <span className="text-chrome text-ink-mute">
              both halves: disabled button + a progress line, never one alone
            </span>
          )}
        </Row>
      </Section>

      <Section title="MethodLabel — verb → token (T2)">
        <Row label="mapped verbs">
          {["GET", "POST", "PUT", "DELETE", "PATCH"].map((m) => (
            <MethodLabel key={m} method={m} />
          ))}
        </Row>
        <Row label="unmapped — body tone">
          {["HEAD", "OPTIONS", "CONNECT", "TRACE", "PROPFIND"].map((m) => (
            <MethodLabel key={m} method={m} />
          ))}
        </Row>
        <Row label="the alignment contract">
          <div className="flex flex-col gap-1">
            {["GET", "OPTIONS", "PUT", "CONNECT"].map((m) => (
              <span key={m} className="flex gap-3">
                <MethodLabel method={m} />
                <span className="font-mono text-body text-ink-mute">/v2/orders/8f3a91c2</span>
              </span>
            ))}
          </div>
        </Row>
        <Row label="lowercase in, uppercase out">
          <MethodLabel method="get" />
        </Row>
      </Section>

      <Section title="StatusLine — a resolved verdict (T2)">
        <Row label="the four settled glyphs">
          <div className="flex flex-col gap-1">
            <StatusLine status={{ glyph: "✓", tone: "ok", message: "Trusted on this machine" }} />
            <StatusLine status={{ glyph: "⚠", tone: "warn", message: "Install failed — declined" }} />
            <StatusLine status={{ glyph: "◐", tone: "muted", message: "Can't check automatically" }} />
            <StatusLine status={{ glyph: "○", tone: "muted", message: "Not trusted yet" }} />
          </div>
        </Row>
        <Row label="in-flight — a spinner">
          <StatusLine status={{ glyph: "⟳", tone: "muted", message: "Installing…" }} />
        </Row>
        <Row label="with the name slot">
          <div className="flex flex-col gap-1">
            <StatusLine name="Certificate" status={{ glyph: "✓", tone: "ok", message: "Trusted" }} />
            <StatusLine name="System proxy" status={{ glyph: "○", tone: "muted", message: "Not set" }} />
          </div>
        </Row>
        <Row label="a long message wraps">
          <StatusLine
            name="Certificate"
            status={trustStatus("untrusted", { decrypted: false, trafficSeen: true }, null)}
          />
        </Row>
        <Row label="precedence — ✓ beats a stale error">
          <div className="flex flex-col gap-1">
            <StatusLine
              name="probe says trusted"
              status={trustStatus("trusted", { decrypted: false, trafficSeen: false }, null, "install exploded")}
            />
            <StatusLine
              name="traffic proves it"
              status={proxyStatus("unknown", { decrypted: true, trafficSeen: true }, null, "set failed")}
            />
          </div>
        </Row>
      </Section>

      <Section title="BodyRenderer family — Json + Xml (the CodeMirror surface)">
        <Row label="JsonRenderer">
          <div className="min-w-0 flex-1">
            <JsonRenderer text={JSON.stringify(JSON.parse(JSON_BODY), null, 2)} />
          </div>
        </Row>
        <Row label="unparseable → as-is">
          <div className="min-w-0 flex-1">
            <JsonRenderer text={'{"broken":true,'} />
          </div>
        </Row>
        <Row label="XmlRenderer">
          <div className="min-w-0 flex-1">
            <XmlRenderer text={XML_BODY} />
          </div>
        </Row>
        <Row label="html down the same path">
          <div className="min-w-0 flex-1">
            <XmlRenderer text={"<div>\n  <p>Order confirmed</p>\n</div>"} />
          </div>
        </Row>
      </Section>

      <Section title="BodyRenderer family — Text · Form · Hex · Image">
        <Row label="TextRenderer">
          <div className="min-w-0 flex-1">
            <TextRenderer text={"Request failed: upstream timed out after 30000ms\n  at Gateway.forward (/srv/gateway/lib/forward.js:118:11)\n  at async Router.handle (/srv/gateway/lib/router.js:64:5)"} />
          </div>
        </Row>
        <Row label="FormRenderer — KeyValueRow×">
          <div className="min-w-0 flex-1">
            <FormRenderer text="grant_type=authorization_code&client_id=app_8f3a91c2&redirect_uri=https%3A%2F%2Fexample.com%2Fcallback&code=SplxlOBeZQQYbYS6WxSbIA" />
          </div>
        </Row>
        <Row label="HexRenderer">
          <div className="min-w-0 flex-1">
            <HexRenderer bytes={PNG_BYTES} />
          </div>
        </Row>
        <Row label="ImageRenderer">
          <ImageRenderer bytes={PNG_BYTES} contentType="image/png" />
        </Row>
        <Row label="CutMarker, alone">
          <CutMarker shown={4096} total={148_480} />
        </Row>
      </Section>

      <div className="h-12" />
    </div>
  );
}
