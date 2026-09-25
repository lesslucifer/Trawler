import { useEffect, useRef } from "react";
import {
  addRow,
  binarySentence,
  bodyRevertible,
  canSend,
  coreOwned,
  droppedRows,
  isEdited,
  removedCount,
  removeRow,
  restart,
  revertBody,
  revertRow,
  rowMark,
  seedBody,
  shownValue,
  updateRow,
  type Draft,
  type DraftRow,
} from "./draft";
import type { Outcome } from "./useDraft";
import {
  Button,
  DurationText,
  Field,
  HTTP_METHODS,
  IconButton,
  MethodLabel,
  Notice,
  Section,
  Select,
  StatusLine,
  StatusPill,
  TimestampText,
  type SelectOption,
} from "./components/index";

export const COMPOSER_COPY = {
  fromFlow: "draft from the flow at",
  fromScratch: "new request",
  startOver: "start over",
  backToFlow: "back to the flow",
  close: "close",
  edited: "edited",
  revert: "revert",
  added: "added",
  follows: "follows the URL",
  addHeader: "add header",
  notInDraft: "not in the draft:",
  noHeaders: "none",
  bodyPlaceholder: "none",
  typeToReplace: "type a body to replace it",
  typeToSend: "type a body to send one",
  typedInstead: "the typed body is sent instead",
  send: "send",
  notSent: "not sent yet",
  needsUrl: "a request needs a URL",
  sending: "sending…",
  openNewFlow: "open the new flow",
} as const;

type Edit = (update: (draft: Draft) => Draft) => void;

function methodOptions(current: string): SelectOption[] {
  const methods = HTTP_METHODS.includes(current) ? HTTP_METHODS : [...HTTP_METHODS, current];
  return methods.map((m) => ({ id: m, label: m, element: <MethodLabel method={m} /> }));
}

// Focus is asked for by name and taken after the render that creates its
// target: an added row, a restored value and a re-enabled send button do not
// exist yet in the handler that wants them focused.
function useFocusAfterRender() {
  const root = useRef<HTMLDivElement>(null);
  const pending = useRef<string | null>(null);
  useEffect(() => {
    if (pending.current === null) return;
    const holder = root.current?.querySelector<HTMLElement>(`[data-focus="${pending.current}"]`);
    pending.current = null;
    holder?.querySelector<HTMLElement>("input, textarea, button")?.focus();
  });
  return { root, focus: (name: string) => void (pending.current = name) };
}

export function Composer({
  draft,
  outcome,
  onEdit,
  onSend,
  onClose,
  onOpenFlow,
}: {
  draft: Draft;
  outcome: Outcome;
  onEdit: Edit;
  onSend: () => void;
  onClose: () => void;
  onOpenFlow: (id: string) => void;
}) {
  const { root, focus } = useFocusAfterRender();

  useEffect(() => {
    root.current?.querySelector<HTMLElement>('[data-focus="url"] input')?.focus();
  }, [root]);

  // A disabled button drops focus to <body>, so the screen hands it back to
  // send when the answer lands — unless the user has already put it elsewhere.
  const phase = useRef(outcome.phase);
  useEffect(() => {
    const was = phase.current;
    phase.current = outcome.phase;
    const landed = was === "sending" && outcome.phase !== "sending";
    if (landed && (document.activeElement === null || document.activeElement === document.body)) {
      root.current?.querySelector<HTMLElement>('[data-focus="send"] button')?.focus();
    }
  }, [outcome.phase, root]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) onClose();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    <div ref={root} data-testid="composer" className="flex h-full flex-col font-ui text-ink">
      <Identity
        draft={draft}
        onEdit={onEdit}
        onClose={onClose}
        onStartOver={() => {
          onEdit(restart);
          focus("url");
        }}
      />
      <div className="min-h-0 flex-1 overflow-auto">
        <Headers draft={draft} onEdit={onEdit} focus={focus} />
        <Body draft={draft} onEdit={onEdit} focus={focus} />
      </div>
      <SendZone draft={draft} outcome={outcome} onSend={onSend} onOpenFlow={onOpenFlow} />
    </div>
  );
}

function Identity({
  draft,
  onEdit,
  onClose,
  onStartOver,
}: {
  draft: Draft;
  onEdit: Edit;
  onClose: () => void;
  onStartOver: () => void;
}) {
  return (
    <div className="shrink-0 border-b border-hairline">
      <div className="flex items-center gap-2 px-4 pt-3">
        <Select
          label="method"
          searchable={false}
          options={methodOptions(draft.method)}
          value={draft.method}
          onChange={(method) => onEdit((d) => ({ ...d, method }))}
        />
        <span data-wire="url" data-focus="url" className="min-w-0 flex-1">
          <Field
            verbatim
            aria-label="url"
            placeholder="https://…"
            className="w-full"
            value={draft.url}
            onChange={(e) => onEdit((d) => ({ ...d, url: e.target.value }))}
          />
        </span>
        <IconButton
          glyph="close"
          label={draft.seed ? COMPOSER_COPY.backToFlow : COMPOSER_COPY.close}
          onClick={onClose}
        />
      </div>
      <div
        data-testid="provenance"
        className="flex min-h-control items-center gap-2 px-4 pb-2 text-chrome text-ink-mute"
      >
        {draft.seed ? (
          <span className="flex items-center gap-chrome-gap">
            {COMPOSER_COPY.fromFlow} <TimestampText timestamp={draft.seed.started_at} />
          </span>
        ) : (
          <span>{COMPOSER_COPY.fromScratch}</span>
        )}
        {isEdited(draft) && (
          <Button frame="bare" tone="mute" className="ml-auto" onClick={onStartOver}>
            {COMPOSER_COPY.startOver}
          </Button>
        )}
      </div>
    </div>
  );
}

function Headers({ draft, onEdit, focus }: { draft: Draft; onEdit: Edit; focus: (name: string) => void }) {
  const removed = removedCount(draft);
  const dropped = draft.seed ? droppedRows(draft.seed) : [];

  const remove = (key: number) => {
    const at = draft.rows.findIndex((r) => r.key === key);
    const next = draft.rows[at + 1];
    onEdit((d) => removeRow(d, key));
    focus(next ? `name-${next.key}` : "add");
  };

  const add = () => {
    const { key } = addRow(draft);
    onEdit((d) => addRow(d).draft);
    focus(`name-${key}`);
  };

  return (
    <Section title={`Headers · ${draft.rows.length}${removed > 0 ? ` · ${removed} removed` : ""}`}>
      {draft.rows.length === 0 && <Notice>{COMPOSER_COPY.noHeaders}</Notice>}
      <div
        data-wire="collection"
        data-order="wire"
        className={
          // design-allow: grid-cols-[auto_1fr_auto_auto] — a four-column template shape, no size in it
          "grid grid-cols-[auto_1fr_auto_auto] items-center gap-x-2 gap-y-chrome-y"
        }
      >
        {draft.rows.map((row, index) => (
          <HeaderRow
            key={row.key}
            draft={draft}
            row={row}
            index={index}
            onEdit={onEdit}
            onRevert={() => {
              onEdit((d) => revertRow(d, row.key));
              focus(`value-${row.key}`);
            }}
            onRemove={() => remove(row.key)}
          />
        ))}
      </div>
      <div data-focus="add" className="pt-2">
        <Button frame="bare" tone="mute" onClick={add}>
          {COMPOSER_COPY.addHeader}
        </Button>
      </div>
      {dropped.length > 0 && (
        <p data-testid="not-in-draft" className="pt-2 text-chrome text-ink-mute">
          {COMPOSER_COPY.notInDraft}{" "}
          {dropped.map((h, i) => (
            <span key={`${h.name}-${i}`}>
              {i > 0 && " · "}
              <span data-wire="header name" className="font-mono">
                {h.name}
              </span>{" "}
              — {coreOwned(h.name)}
            </span>
          ))}
        </p>
      )}
    </Section>
  );
}

function HeaderRow({
  draft,
  row,
  index,
  onEdit,
  onRevert,
  onRemove,
}: {
  draft: Draft;
  row: DraftRow;
  index: number;
  onEdit: Edit;
  onRevert: () => void;
  onRemove: () => void;
}) {
  const mark = rowMark(draft, row);
  return (
    <div data-testid="header-row" data-mark={mark ?? undefined} className="contents">
      <span data-wire="header name" data-focus={`name-${row.key}`}>
        <Field
          verbatim
          aria-label={`header ${index + 1} name`}
          className="w-field"
          value={row.name}
          onChange={(e) => onEdit((d) => updateRow(d, row.key, { name: e.target.value }))}
        />
      </span>
      <span data-wire="header value" data-focus={`value-${row.key}`} className="min-w-0">
        <Field
          verbatim
          aria-label={`${row.name || `header ${index + 1}`} value`}
          className="w-full"
          value={shownValue(draft, row)}
          onChange={(e) => onEdit((d) => updateRow(d, row.key, { value: e.target.value }))}
        />
      </span>
      <span data-testid="row-mark" className="flex items-center gap-chrome-gap text-chrome text-ink-mute">
        {mark === "edited" && (
          <>
            {COMPOSER_COPY.edited}
            <Button frame="bare" tone="mute" onClick={onRevert}>
              {COMPOSER_COPY.revert}
            </Button>
          </>
        )}
        {mark === "added" && COMPOSER_COPY.added}
        {mark === "follows" && COMPOSER_COPY.follows}
      </span>
      <IconButton glyph="close" label={`remove ${row.name || `header ${index + 1}`}`} onClick={onRemove} />
    </div>
  );
}

function Body({ draft, onEdit, focus }: { draft: Draft; onEdit: Edit; focus: (name: string) => void }) {
  const seeded = draft.seed ? seedBody(draft.seed.request_body) : null;
  const typed = draft.body !== "";
  const standIn =
    seeded?.kind === "absent"
      ? { sentence: seeded.sentence, detail: COMPOSER_COPY.typeToSend }
      : seeded?.kind === "binary"
        ? { sentence: binarySentence(seeded.size), detail: COMPOSER_COPY.typeToReplace }
        : null;

  return (
    <Section title="Body">
      {standIn && (
        <div data-testid="body-stand-in" className="pb-2">
          <Notice detail={typed ? COMPOSER_COPY.typedInstead : standIn.detail}>{standIn.sentence}</Notice>
        </div>
      )}
      <span data-wire="request body" data-focus="body" className="block">
        <Field
          multiline
          verbatim
          aria-label="body"
          placeholder={COMPOSER_COPY.bodyPlaceholder}
          className="h-body-max w-full"
          value={draft.body}
          onChange={(e) => onEdit((d) => ({ ...d, body: e.target.value }))}
        />
      </span>
      {bodyRevertible(draft) && (
        <p data-testid="body-mark" className="flex items-center gap-chrome-gap pt-chrome-y text-chrome text-ink-mute">
          {COMPOSER_COPY.edited}
          <Button
            frame="bare"
            tone="mute"
            onClick={() => {
              onEdit(revertBody);
              focus("body");
            }}
          >
            {COMPOSER_COPY.revert}
          </Button>
        </p>
      )}
    </Section>
  );
}

function SendZone({
  draft,
  outcome,
  onSend,
  onOpenFlow,
}: {
  draft: Draft;
  outcome: Outcome;
  onSend: () => void;
  onOpenFlow: (id: string) => void;
}) {
  const sending = outcome.phase === "sending";
  const line = (glyph: string, tone: "muted" | "warn", short: string) => (
    <StatusLine status={{ glyph, tone, short }} className="min-w-0" />
  );
  return (
    <div data-testid="send-zone" className="flex min-h-control shrink-0 items-center gap-3 border-t border-hairline px-4 py-2">
      <span data-focus="send">
        <Button onClick={onSend} disabled={sending || !canSend(draft)}>
          {COMPOSER_COPY.send}
        </Button>
      </span>
      {sending ? (
        line("⟳", "muted", COMPOSER_COPY.sending)
      ) : !canSend(draft) ? (
        line("○", "muted", COMPOSER_COPY.needsUrl)
      ) : outcome.phase === "failed" ? (
        <div data-testid="send-failed" className="min-w-0">
          {line("⚠", "warn", outcome.reason)}
        </div>
      ) : outcome.phase === "sent" ? (
        <>
          <StatusPill status={outcome.flow.status} />
          <DurationText value={outcome.flow.duration_ms} />
          <Button frame="bare" tone="mute" className="ml-auto" onClick={() => onOpenFlow(outcome.flow.id)}>
            {COMPOSER_COPY.openNewFlow}
          </Button>
        </>
      ) : (
        line("○", "muted", COMPOSER_COPY.notSent)
      )}
    </div>
  );
}
