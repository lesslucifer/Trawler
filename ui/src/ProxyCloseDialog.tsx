import { useState } from "react";
import { defaultProxyTarget, proxyOffSteps, type HostOs } from "./setup";
import type { CloseMode } from "./closeConfirm";

// The quit-time proxy dialog (E-UISETUP.W5), two modes driven by the live proxy
// status (see closeConfirm.ts):
//   "remove" — a proxy on THIS system routes through Trawler; Remove (native) or
//              Keep. Quitting silently would strand the machine with no internet.
//   "notice" — the proxy is foreign or not on this system (maybe on another device
//              we can't read); info-only — how to check / turn it off — with a
//              durable "don't show again". No Remove button; nothing local to fix.
// Dismissing (Esc / backdrop / ✕) cancels the quit in either mode.

export const CLOSE_COPY = {
  remove: {
    title: "Remove Trawler's proxy before quitting?",
    body: "A proxy on this machine is routing through Trawler. If you keep it after quitting, your internet may stop working until you turn the proxy off.",
    keep: "Keep",
    confirm: "Remove",
  },
  notice: {
    title: "Check your proxy before quitting",
    body: "Trawler isn't the active proxy on this machine, but a device you pointed at Trawler (a phone or emulator on your network) may still route through it — and lose internet once Trawler closes. To check and turn it off:",
    dontShowAgain: "Don't show this again",
    confirm: "Quit",
  },
} as const;

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
  onRemove: () => void;
  onKeep: () => void;
  onAcknowledge: (dontShowAgain: boolean) => void;
  onCancel: () => void;
}) {
  const [dontShowAgain, setDontShowAgain] = useState(false);
  const copy = CLOSE_COPY[mode];

  return (
    <div
      data-testid="proxy-close-dialog"
      data-mode={mode}
      role="dialog"
      aria-modal="true"
      aria-label={copy.title}
      onKeyDown={(e) => e.key === "Escape" && onCancel()}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    >
      <button
        type="button"
        aria-hidden="true"
        tabIndex={-1}
        data-testid="proxy-close-backdrop"
        onClick={onCancel}
        className="absolute inset-0 h-full w-full cursor-default"
      />
      <div className="relative w-full max-w-sm rounded-lg border border-hairline bg-canvas p-4 text-ink">
        <div className="mb-3 flex items-start justify-between gap-2">
          <h2 className="font-serif text-lg italic">{copy.title}</h2>
          <button
            type="button"
            aria-label="Cancel"
            data-testid="proxy-close-dismiss"
            onClick={onCancel}
            className="text-ink-mute hover:text-ink"
          >
            ✕
          </button>
        </div>
        <p className="mb-3 text-sm text-ink-mute">{copy.body}</p>

        {mode === "notice" && (
          <>
            <ol data-testid="proxy-close-steps" className="mb-3 list-decimal pl-5 text-sm text-ink-mute">
              {proxyOffSteps(defaultProxyTarget(os)).map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
            <label className="mb-3 flex items-center gap-2 text-sm text-ink-mute">
              <input
                type="checkbox"
                data-testid="proxy-close-dont-show"
                checked={dontShowAgain}
                onChange={(e) => setDontShowAgain(e.target.checked)}
              />
              {CLOSE_COPY.notice.dontShowAgain}
            </label>
          </>
        )}

        <div className="flex justify-end gap-2">
          {mode === "remove" ? (
            <>
              <button
                type="button"
                data-testid="proxy-close-keep"
                onClick={onKeep}
                className="rounded-button border border-hairline px-3 py-1 text-ink-mute hover:text-ink"
              >
                {CLOSE_COPY.remove.keep}
              </button>
              <button
                type="button"
                data-testid="proxy-close-remove"
                onClick={onRemove}
                className="rounded-button border border-hairline px-3 py-1 text-ink"
              >
                {CLOSE_COPY.remove.confirm}
              </button>
            </>
          ) : (
            <button
              type="button"
              data-testid="proxy-close-quit"
              onClick={() => onAcknowledge(dontShowAgain)}
              className="rounded-button border border-hairline px-3 py-1 text-ink"
            >
              {CLOSE_COPY.notice.confirm}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
