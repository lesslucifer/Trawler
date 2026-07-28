import { useState } from "react";
import {
  Button,
  CopyButton,
  CopyText,
  DurationText,
  IconButton,
  KeyValueRow,
  Notice,
  Section,
  SegmentedControl,
  Select,
  StatusPill,
  StepList,
  TimestampText,
  TonedText,
} from "./components/index";

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

      <div className="h-12" />
    </div>
  );
}
