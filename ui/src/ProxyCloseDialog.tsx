import { useRef, useState } from "react";
import {
  defaultProxyTarget,
  proxyOffSnippet,
  proxyOffSteps,
  PROXY_TARGETS,
  type ProxyTarget,
  type HostOs,
} from "./setup";
import type { CloseMode } from "./closeConfirm";
import { Button, Checkbox, Notice } from "./components/index";
import { ModalShell } from "./ModalShell";
import { ManualFallback, ManualSteps, TargetPicker } from "./manualSteps";

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
  done: "the proxy is off. you can quit safely.",
  failed: "couldn't turn the proxy off — try the steps below.",
} as const;

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

  return (
    <ManualFallback
      summary="do it manually instead"
      collapsed={collapsed}
      defaultOpen={defaultOpen}
      testId="proxy-close-manual"
      className="mb-3"
    >
      {body}
    </ManualFallback>
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
  const actions = useRef<HTMLDivElement>(null);
  const [dontShowAgain, setDontShowAgain] = useState(false);
  const [remedyState, setRemedyState] = useState<"idle" | "running" | "done" | "failed">("idle");
  const failed = remedyState === "failed";
  const running = remedyState === "running";
  const done = remedyState === "done";
  const copy = CLOSE_COPY[mode];

  if (mode === "remote") {
    return (
      <ModalShell
        title={copy.title}
        onCancel={onCancel}
        testId="proxy-close"
        mode={mode}
      >
        <p className="mb-3 font-ui text-body text-ink-mute">{copy.body}</p>
        <ProxyOffSteps os={os} collapsed={false} />
        <div className="mb-3">
          <Checkbox checked={dontShowAgain} onChange={setDontShowAgain}>
            {CLOSE_COPY.remote.dontShowAgain}
          </Checkbox>
        </div>
        <div className="flex justify-end">
          <span data-opener="true">
            <Button onClick={() => onAcknowledge(dontShowAgain)} frame="bare" tone="mute">
              {CLOSE_COPY.quit}
            </Button>
          </span>
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
    // The remedy is about to be disabled; a browser blurs a focused control when
    // that happens, dropping focus outside the panel where Tab escapes the modal.
    // Hand focus to the exit before it goes.
    const row = actions.current;
    if (row?.contains(document.activeElement)) {
      row.querySelector<HTMLElement>('[data-exit="true"] button')?.focus();
    }
    setRemedyState("running");
    try {
      Promise.resolve(onRemove()).then(
        () => setRemedyState("done"),
        () => setRemedyState("failed"),
      );
    } catch {
      setRemedyState("failed");
    }
  };

  return (
    <ModalShell
      title={copy.title}
      onCancel={onCancel}
      testId="proxy-close"
      mode={mode}
    >
      <p className="mb-3 font-ui text-body text-ink-mute">{copy.body}</p>
      {running && (
        <div className="mb-2" data-testid="proxy-close-running">
          <Notice>{CLOSE_COPY.turningOff}</Notice>
        </div>
      )}
      {done && (
        <div className="mb-2" data-testid="proxy-close-done">
          <Notice tone="ok">{CLOSE_COPY.done}</Notice>
        </div>
      )}
      {failed && (
        <div className="mb-2" data-testid="proxy-close-failed">
          <Notice tone="warn">{CLOSE_COPY.failed}</Notice>
        </div>
      )}
      <ProxyOffSteps os={os} collapsed defaultOpen={failed} />
      <div ref={actions} className="flex justify-end gap-chrome-gap">
        <span data-exit="true">
          <Button onClick={onKeep} frame="bare" tone="mute">
            {CLOSE_COPY.quit}
          </Button>
        </span>
        <span data-opener="true">
          <Button onClick={attemptRemove} disabled={running || done}>
            {failed ? CLOSE_COPY.tryAgain : CLOSE_COPY.turnOff}
          </Button>
        </span>
      </div>
    </ModalShell>
  );
}
