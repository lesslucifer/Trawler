import { useEffect, useRef, useState } from "react";
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
import { HEX_MAX, TEXT_MAX, TREE_MAX } from "./body";
import { proxyStatus, trustStatus, verifyStatus } from "./setup";

// ponytail: throwaway verification screen for E-DFLOW.W5 step 4 — every T1
// component in every state its design file names, so a human can eyeball what
// the unit tests assert. Delete this file and its two lines in main.tsx once the
// collection is signed off; nothing in the app imports it.

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-start gap-4 border-b border-hairline/hairline py-2 last:border-0">
      <span className="w-pane-narrow shrink-0 pt-chrome-tight font-mono text-label uppercase tracking-label text-ink-mute">
        {label}
      </span>
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-3">{children}</div>
    </div>
  );
}

// Body panes get a bounded, scrollable box — a capped body is thousands of
// lines, and an unbounded one would bury every section below it. `tail` starts
// the box scrolled to the bottom, which is the only way a cut marker sitting
// after 128 KB of text is visible without hand-scrolling every one.
function Pane({ children, tail = false }: { children: React.ReactNode; tail?: boolean }) {
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (tail && box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [tail]);

  return (
    <div
      ref={box}
      className="min-w-0 max-h-body-max flex-1 overflow-auto rounded-card border border-hairline bg-canvas-soft/wash p-2"
    >
      {children}
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

// Fully opaque, so the checkerboard is hidden — the contrast that shows the
// transparency case above is doing something.
const OPAQUE_PNG_BASE64 =
  "iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAYAAAAf8/9hAAAAJ0lEQVR42mPwOdz1Hxn377iHggnJMwwDA0jVgC4/HAwYTQej6QCIAXXe3h8PphyMAAAAAElFTkSuQmCC";

const bytesFrom = (base64: string) => Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));

const PNG_BYTES = bytesFrom(PNG_BASE64);
const OPAQUE_PNG_BYTES = bytesFrom(OPAQUE_PNG_BASE64);

// Over TEXT_MAX / TREE_MAX / HEX_MAX, so each member draws its rendered+cut
// state — the family's second state, and the one no fixture above reaches.
const OVERSIZE_TEXT = `id,sku,qty,price\n${"1,p1,2,44.50\n".repeat(Math.ceil(TEXT_MAX / 13) + 1)}`;
const OVERSIZE_JSON = `{"pad":"${"a".repeat(TREE_MAX)}"}`;
const OVERSIZE_XML = `<order>\n  <note>${"x".repeat(TEXT_MAX)}</note>\n</order>`;
const OVERSIZE_FORM = `token=${"z".repeat(TEXT_MAX)}&trailing=cut`;
const OVERSIZE_BYTES = new Uint8Array(HEX_MAX + 10_240).map((_, i) => (i * 7) % 256);

const NOTHING_SEEN = { decrypted: false, trafficSeen: false };

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
      <header className="sticky top-0 z-sticky border-b border-hairline bg-canvas px-4 py-3">
        <h1 className="font-serif text-title italic">Component gallery</h1>
        <p className="mt-chrome-y text-chrome text-ink-mute">
          E-DFLOW.W5 steps 4 + 5 — every T1 and T2 component, every state its
          design file names. Throwaway screen.
        </p>
        {fired && (
          <p className="mt-chrome-y font-mono text-chrome text-ok">handler fired → {fired}</p>
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
          <span className="text-title">
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
          <div className="w-pane-label border border-hairline/scrim p-2">
            <CopyText value={URI} label="request URI" />
          </div>
        </Row>
      </Section>

      <Section title="KeyValueRow — one wire pair">
        <dl
          className={
            // design-allow: grid-cols-[auto_1fr] — a two-column template shape, no size in it
            "grid grid-cols-[auto_1fr] gap-x-grid-gap-x gap-y-grid-gap-y"
          }
        >
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
          <span className="border border-hairline/scrim">
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
        <Row label="the alignment contract — all nine, left edges align">
          <div className="flex flex-col gap-chrome-y">
            {["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "CONNECT", "TRACE"].map(
              (m) => (
                <span key={m} className="flex gap-3">
                  <MethodLabel method={m} />
                  <span className="font-mono text-body text-ink-mute">/v2/orders/8f3a91c2</span>
                </span>
              ),
            )}
          </div>
        </Row>
        <Row label="lowercase in, uppercase out">
          <MethodLabel method="get" />
          <MethodLabel method="Post" />
          <MethodLabel method="dElEtE" />
        </Row>
        <Row label="a long non-standard verb overflows rather than truncating">
          <span className="flex gap-3">
            <MethodLabel method="PROPPATCH" />
            <span className="font-mono text-body text-ink-mute">/dav/calendar</span>
          </span>
        </Row>
      </Section>

      <Section title="StatusLine — a resolved verdict (T2)">
        <Row label="the four settled glyphs">
          <div className="flex flex-col gap-chrome-y">
            <StatusLine status={{ glyph: "✓", tone: "ok", short: "trusted on this machine" }} />
            <StatusLine status={{ glyph: "⚠", tone: "warn", short: "install failed" }} />
            <StatusLine status={{ glyph: "◐", tone: "muted", short: "can't check automatically" }} />
            <StatusLine status={{ glyph: "○", tone: "muted", short: "not trusted yet" }} />
          </div>
        </Row>
        <Row label="in-flight — a spinner, and it outranks everything">
          <div className="flex flex-col gap-chrome-y">
            <StatusLine status={{ glyph: "⟳", tone: "muted", short: "installing…" }} />
            <StatusLine
              name="over a stale error"
              status={trustStatus("untrusted", NOTHING_SEEN, "installing", "the last install failed")}
            />
            <StatusLine
              name="over a proven positive"
              status={proxyStatus("ours", { decrypted: true, trafficSeen: true }, "setting")}
            />
          </div>
        </Row>
        <Row label="the two muted glyphs are different verdicts">
          <div className="flex flex-col gap-chrome-y">
            <StatusLine
              name="◐ can't determine"
              status={trustStatus("unknown", NOTHING_SEEN, null)}
            />
            <StatusLine name="○ not yet" status={trustStatus("untrusted", NOTHING_SEEN, null)} />
          </div>
        </Row>
        <Row label="verifyStatus — the third resolver">
          <div className="flex flex-col gap-chrome-y">
            <StatusLine name="Capture" status={verifyStatus({ decrypted: true, trafficSeen: true })} />
            <StatusLine name="Capture" status={verifyStatus(NOTHING_SEEN)} />
          </div>
        </Row>
        <Row label="a foreign proxy — the longest real message in the app">
          <StatusLine name="System proxy" status={proxyStatus("foreign", NOTHING_SEEN, null)} />
        </Row>
        <Row label="with the name slot">
          <div className="flex flex-col gap-chrome-y">
            <StatusLine name="Certificate" status={{ glyph: "✓", tone: "ok", short: "trusted" }} />
            <StatusLine name="System proxy" status={{ glyph: "○", tone: "muted", short: "not set" }} />
          </div>
        </Row>
        <Row label="a long message wraps">
          <StatusLine
            name="Certificate"
            status={trustStatus("untrusted", { decrypted: false, trafficSeen: true }, null)}
          />
        </Row>
        <Row label="precedence — ✓ beats a stale error">
          <div className="flex flex-col gap-chrome-y">
            <StatusLine
              name="probe says trusted"
              status={trustStatus("trusted", NOTHING_SEEN, null, "install exploded")}
            />
            <StatusLine
              name="traffic proves it"
              status={proxyStatus("unknown", { decrypted: true, trafficSeen: true }, null, "set failed")}
            />
          </div>
        </Row>
      </Section>

      <Section title="JsonRenderer — the CodeMirror surface (T2)">
        <Row label="rendered — click the ▾ gutter, arrow around, select and copy">
          <Pane>
            <JsonRenderer text={JSON.stringify(JSON.parse(JSON_BODY), null, 2)} />
          </Pane>
        </Row>
        <Row label="every token: key · string · number · punctuation">
          <Pane>
            <JsonRenderer
              text={'{\n  "key": "string",\n  "number": 42,\n  "float": 49.99,\n  "yes": true,\n  "nothing": null\n}'}
            />
          </Pane>
        </Row>
        <Row label="as-is fallback — unparseable, so no tokens and no surface">
          <Pane>
            <JsonRenderer text={'{"id":"8f3a91c2","items":[{"sku":"p1","qty":2}],"total":89'} />
          </Pane>
        </Row>
        <Row label="as-is fallback — over TREE_MAX, with the cut marker">
          <Pane tail>
            <JsonRenderer text={OVERSIZE_JSON} />
          </Pane>
        </Row>
      </Section>

      <Section title="XmlRenderer — the same surface, folded by element (T2)">
        <Row label="rendered — tag · attr name · attr value · element text">
          <Pane>
            <XmlRenderer text={XML_BODY} />
          </Pane>
        </Row>
        <Row label="html down the same path (text/html)">
          <Pane>
            <XmlRenderer text={"<div class=\"card\">\n  <p>Order confirmed</p>\n</div>"} />
          </Pane>
        </Row>
        <Row label="rendered + cut — over TEXT_MAX, falls back to text">
          <Pane tail>
            <XmlRenderer text={OVERSIZE_XML} />
          </Pane>
        </Row>
      </Section>

      <Section title="TextRenderer — the family's floor (T2)">
        <Row label="rendered — verbatim, wrapping, no tokens">
          <Pane>
            <TextRenderer text={"Request failed: upstream timed out after 30000ms\n  at Gateway.forward (/srv/gateway/lib/forward.js:118:11)\n  at async Router.handle (/srv/gateway/lib/router.js:64:5)"} />
          </Pane>
        </Row>
        <Row label="a long unbroken line wraps rather than scrolling the panel">
          <Pane>
            <TextRenderer text={"eyJhbGciOiJIUzI1NiJ9.".repeat(24)} />
          </Pane>
        </Row>
        <Row label="rendered + cut — the cap this component's spec added">
          <Pane tail>
            <TextRenderer text={OVERSIZE_TEXT} />
          </Pane>
        </Row>
      </Section>

      <Section title="FormRenderer — decoded pairs, composed (T2)">
        <Row label="rendered — click any value to copy it whole">
          <Pane>
            <FormRenderer text="grant_type=authorization_code&client_id=app_8f3a91c2&redirect_uri=https%3A%2F%2Fexample.com%2Fcallback&code=SplxlOBeZQQYbYS6WxSbIA" />
          </Pane>
        </Row>
        <Row label="percent- and plus-decoding">
          <Pane>
            <FormRenderer text="q=two+words&path=%2Fv2%2Forders&note=50%25+off" />
          </Pane>
        </Row>
        <Row label="a repeated key appears once per occurrence">
          <Pane>
            <FormRenderer text="tag=rush&tag=gift&tag=fragile" />
          </Pane>
        </Row>
        <Row label="rendered + cut">
          <Pane tail>
            <FormRenderer text={OVERSIZE_FORM} />
          </Pane>
        </Row>
      </Section>

      <Section title="HexRenderer — offset · bytes · ASCII (T2)">
        <Row label="rendered — a real PNG header">
          <Pane>
            <HexRenderer bytes={PNG_BYTES} />
          </Pane>
        </Row>
        <Row label="non-printable bytes become dots">
          <Pane>
            <HexRenderer bytes={new Uint8Array([0x00, 0x01, 0x1f, 0x7f, 0x41, 0x42, 0x43])} />
          </Pane>
        </Row>
        <Row label="rendered + cut — over HEX_MAX">
          <Pane tail>
            <HexRenderer bytes={OVERSIZE_BYTES} />
          </Pane>
        </Row>
      </Section>

      <Section title="ImageRenderer — letterboxed preview (T2)">
        <Row label="transparent — the checkerboard shows through">
          <ImageRenderer bytes={PNG_BYTES} contentType="image/png" />
        </Row>
        <Row label="opaque — the same checkerboard, hidden">
          <ImageRenderer bytes={OPAQUE_PNG_BYTES} contentType="image/png" />
        </Row>
        <Row label="no content type — the caption drops the MIME, nothing breaks">
          <ImageRenderer bytes={PNG_BYTES} contentType={null} />
        </Row>
        <Row label="never a cut marker — the element renders every byte">
          <ImageRenderer bytes={PNG_BYTES} contentType="image/svg+xml" />
        </Row>
      </Section>

      <Section title="CutMarker — the family's truncation statement (T2)">
        <Row label="B → KiB → MiB, one decimal below 10">
          <div className="flex min-w-0 flex-1 flex-col">
            <CutMarker shown={512} total={4096} />
            <CutMarker shown={4096} total={14_848} />
            <CutMarker shown={4096} total={148_480} />
            <CutMarker shown={131_072} total={1_048_576} />
            <CutMarker shown={131_072} total={15_728_640} />
          </div>
        </Row>
      </Section>

      <div className="h-12" />
    </div>
  );
}
