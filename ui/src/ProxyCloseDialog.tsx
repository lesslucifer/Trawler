import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  defaultProxyTarget,
  proxyOffSnippet,
  proxyOffSteps,
  PROXY_TARGETS,
  type ProxyTarget,
  type HostOs,
} from "./setup";
import type { CloseMode } from "./closeConfirm";
import { Button, Checkbox, Disclosure, IconButton, Notice } from "./components/index";
import { ManualSteps, TargetPicker } from "./manualSteps";

// The quit-time proxy dialog, three modes driven by the live proxy status
// (see closeConfirm.ts):
//   "ours" / "foreign" — a proxy on THIS machine is set. `unset_proxy` turns the
//       OS proxy state off whoever set it, so both are actionable and differ only
//       in wording. Quitting silently would strand the machine with no internet.
//   "remote" — nothing is set here, but a device pointed at Trawler may still
//       route through it. Info-only, and the one mode that may offer to stop
//       asking; there is nothing local to fix.
// Dismissing (Esc / backdrop / ✕) cancels the quit in every mode.

export const CLOSE_COPY = {
  ours: {
    title: "turn the proxy off before quitting?",
    body: "A proxy on this machine routes through Trawler. Leave it on after quitting and your internet may stop working until you turn it off.",
  },
  foreign: {
    title: "turn the proxy off before quitting?",
    body: "A proxy is set on this machine. It isn't Trawler's, but if it points here it will stop working once Trawler closes.",
  },
  remote: {
    title: "check your other devices before quitting",
    body: "No proxy is set on this machine, but a device you pointed at Trawler may still route through it — and lose internet once Trawler closes.",
    dontShowAgain: "don't show this again",
  },
  quit: "quit",
  turnOff: "turn it off",
  tryAgain: "try again",
  turningOff: "turning the proxy off — approve the prompt if asked…",
  failed: "couldn't turn the proxy off — try the steps below.",
} as const;

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

// Build-local by warrant: no registry entry owns modality, and a general Dialog
// drawn from the app's only dialog would generalise from a sample of one.
function ModalShell({
  title,
  onCancel,
  children,
  testId,
  mode,
}: {
  title: string;
  onCancel: () => void;
  children: ReactNode;
  testId: string;
  mode: string;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const invoker = useRef<Element | null>(null);

  useEffect(() => {
    invoker.current = document.activeElement;
    panel.current?.querySelector<HTMLElement>(FOCUSABLE)?.focus();
    return () => {
      (invoker.current as HTMLElement | null)?.focus?.();
    };
  }, []);

  const trap = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") {
      onCancel();
      return;
    }
    if (event.key !== "Tab" || !panel.current) return;
    const stops = [...panel.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
    if (stops.length === 0) return;
    const edge = event.shiftKey ? stops[0] : stops[stops.length - 1];
    if (document.activeElement === edge) {
      event.preventDefault();
      (event.shiftKey ? stops[stops.length - 1] : stops[0]).focus();
    }
  };

  return (
    <div
      data-testid={testId}
      data-mode={mode}
      role="dialog"
      aria-modal="true"
      aria-label={title}
      onKeyDown={trap}
      className="fixed inset-0 z-modal flex items-center justify-center bg-scrim"
    >
      <button
        type="button"
        aria-hidden="true"
        tabIndex={-1}
        data-testid="proxy-close-backdrop"
        onClick={onCancel}
        className="absolute inset-0 h-full w-full cursor-default"
      />
      <div
        ref={panel}
        className="relative w-full max-w-dialog-max rounded-lg border border-hairline bg-canvas p-4 text-ink"
      >
        <div className="mb-3 flex items-start justify-between gap-2">
          <h2 className="font-serif text-title italic">{title}</h2>
          <IconButton glyph="close" label="cancel" onClick={onCancel} />
        </div>
        {children}
      </div>
    </div>
  );
}

// Shared by every mode: pick the target, read the steps, copy the command. It is
// the whole content in "remote" (nothing to act on) and the fallback beneath the
// action in "ours"/"foreign".
function ProxyOffSteps({
  os,
  collapsed,
  defaultOpen,
}: {
  os: HostOs;
  collapsed: boolean;
  defaultOpen?: boolean;
}) {
  const [target, setTarget] = useState<ProxyTarget>(defaultProxyTarget(os));

  const body = (
    <>
      <TargetPicker
        options={PROXY_TARGETS}
        value={target}
        onChange={(id) => setTarget(id as ProxyTarget)}
        label="proxy target"
      />
      <ManualSteps
        steps={proxyOffSteps(target)}
        command={proxyOffSnippet(target)}
        commandLabel="proxy-off command"
        testId="proxy-close-steps"
      />
    </>
  );

  if (!collapsed) return <div className="mb-3">{body}</div>;
  return (
    <div className="mb-3" data-testid="proxy-close-manual">
      <Disclosure summary="do it manually instead" defaultOpen={defaultOpen}>
        {body}
      </Disclosure>
    </div>
  );
}

export function ProxyCloseDialog({
  mode,
  os,
  onRemove,
  onKeep,
  onAcknowledge,
  onCancel,
}: {
  mode: CloseMode;
  os: HostOs;
  onRemove: () => Promise<void> | void;
  onKeep: () => void;
  onAcknowledge: (dontShowAgain: boolean) => void;
  onCancel: () => void;
}) {
  const [dontShowAgain, setDontShowAgain] = useState(false);
  const [remedyState, setRemedyState] = useState<"idle" | "running" | "failed">("idle");
  const failed = remedyState === "failed";
  const running = remedyState === "running";
  const copy = CLOSE_COPY[mode];

  if (mode === "remote") {
    return (
      <ModalShell title={copy.title} onCancel={onCancel} testId="proxy-close-dialog" mode={mode}>
        <p className="mb-3 font-ui text-body text-ink-mute">{copy.body}</p>
        <ProxyOffSteps os={os} collapsed={false} />
        <div className="mb-3">
          <Checkbox checked={dontShowAgain} onChange={setDontShowAgain}>
            {CLOSE_COPY.remote.dontShowAgain}
          </Checkbox>
        </div>
        <div className="flex justify-end">
          <Button onClick={() => onAcknowledge(dontShowAgain)} frame="bare" tone="mute">
            {CLOSE_COPY.quit}
          </Button>
        </div>
      </ModalShell>
    );
  }

  // onRemove runs synchronously on the click; only its rejection is awaited, so
  // a handler that throws outright is caught the same as one that rejects. The
  // button is disabled for the whole in-flight window with a progress line beside
  // it (fork D4), since unset_proxy is elevation-gated and slow enough to invite
  // a second click that would fire a second elevation prompt.
  const attemptRemove = () => {
    setRemedyState("running");
    try {
      Promise.resolve(onRemove()).then(
        () => setRemedyState("idle"),
        () => setRemedyState("failed"),
      );
    } catch {
      setRemedyState("failed");
    }
  };

  return (
    <ModalShell title={copy.title} onCancel={onCancel} testId="proxy-close-dialog" mode={mode}>
      <p className="mb-3 font-ui text-body text-ink-mute">{copy.body}</p>
      {running && (
        <div className="mb-2" data-testid="proxy-close-running">
          <Notice>{CLOSE_COPY.turningOff}</Notice>
        </div>
      )}
      {failed && (
        <div className="mb-2" data-testid="proxy-close-failed">
          <Notice tone="warn">{CLOSE_COPY.failed}</Notice>
        </div>
      )}
      <ProxyOffSteps os={os} collapsed defaultOpen={failed} />
      <div className="flex justify-end gap-chrome-gap">
        <Button onClick={onKeep} frame="bare" tone="mute">
          {CLOSE_COPY.quit}
        </Button>
        <Button onClick={attemptRemove} disabled={running}>
          {failed ? CLOSE_COPY.tryAgain : CLOSE_COPY.turnOff}
        </Button>
      </div>
    </ModalShell>
  );
}
