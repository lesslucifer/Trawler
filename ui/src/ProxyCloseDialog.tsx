import { useRef, useState } from "react";
import {
  canAutoSetup,
  deviceName,
  platformChoices,
  routeUndo,
  type HostOs,
  type PlatformId,
} from "./setup";
import type { QuitPrompt } from "./closeConfirm";
import { Button, Checkbox, Notice } from "./components/index";
import { ModalShell } from "./ModalShell";
import { GuideSteps, ManualFallback, TargetPicker } from "./manualSteps";

// The quit-time dialog, built from its design run (E-ONBRVP.W11): one frame for
// every prompt (closeConfirm.ts). The stake leads; the steps are the platform's
// own (setup.ts's routeUndo); the button, where the shell can act, gives way to
// its outcome; an opt-out only where nothing here is verified to act on.
// Dismissing (Esc / backdrop / ✕) cancels the quit in every prompt.

const capitalized = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

// The platforms whose traffic could still be routing here once this machine's
// own proxy is clean: those reach lets in, and the emulator, already inside
// this machine's loopback.
const DEVICES: readonly PlatformId[] = ["mac", "windows", "ubuntu", "android-sim", "ios", "android"];

export const CLOSE_COPY = {
  title: {
    ours: "stop routing through Trawler before quitting?",
    unread: (host: string) => `check ${host}'s proxy before quitting`,
    devices: "check your other devices before quitting",
  },
  stake: {
    ours: (host: string) =>
      `${capitalized(host)} sends its web traffic through Trawler. Once Trawler's capture service stops, ${host} has no internet until you stop routing through Trawler.`,
    stale: (host: string) =>
      `Trawler's capture service isn't answering, so ${host} has no internet while it routes through Trawler.`,
    unread: (host: string) =>
      `Trawler couldn't read ${host}'s proxy. If it points at Trawler, ${host} has no internet once Trawler's capture service stops.`,
    devices:
      "Devices on your network can use Trawler now. Any still set to it have no internet once Trawler's capture service stops.",
    emulator: (name: string) =>
      `${name} sends its traffic through Trawler, and has no internet once Trawler's capture service stops.`,
  },
  byHand: "do it by hand",
  dontShowAgain: "don't show this again",
  quit: "quit",
  stop: "stop routing through Trawler",
  tryAgain: "try again",
  stopping: (os: HostOs) => (os === "macos" ? "stopping — approve the prompt if asked…" : "stopping…"),
  stopped: (host: string) => `${capitalized(host)} no longer routes through Trawler.`,
  failed: "couldn't stop routing through Trawler — the steps below do it by hand.",
} as const;

function stakeOf(prompt: QuitPrompt, host: string): string {
  switch (prompt.mode) {
    case "ours":
      return prompt.answering ? CLOSE_COPY.stake.ours(host) : CLOSE_COPY.stake.stale(host);
    case "unread":
      return CLOSE_COPY.stake.unread(host);
    case "devices":
      return prompt.emulator === null ? CLOSE_COPY.stake.devices : CLOSE_COPY.stake.emulator(prompt.emulator);
  }
}

function titleOf(prompt: QuitPrompt, host: string): string {
  return prompt.mode === "unread" ? CLOSE_COPY.title.unread(host) : CLOSE_COPY.title[prompt.mode];
}

// A device's steps open on the emulator the reading found, else on the device
// the user last set up, else on the commonest phone.
export function deviceShown(prompt: QuitPrompt, chosen: PlatformId): PlatformId {
  if (prompt.mode === "devices" && prompt.emulator !== null) return "android-sim";
  return DEVICES.includes(chosen) ? chosen : "ios";
}

function DeviceSteps({ os, initial }: { os: HostOs; initial: PlatformId }) {
  const [device, setDevice] = useState<PlatformId>(initial);
  const options = platformChoices(os).filter(({ id }) => DEVICES.includes(id));
  return (
    <div data-testid="proxy-close-steps">
      <TargetPicker
        options={options}
        value={device}
        onChange={(id) => setDevice(id as PlatformId)}
        label="steps for"
      />
      <GuideSteps steps={routeUndo(device, os)} />
    </div>
  );
}

export function ProxyCloseDialog({
  prompt,
  os,
  chosen,
  onStop,
  onQuit,
  onCancel,
}: {
  prompt: QuitPrompt;
  os: HostOs;
  chosen: PlatformId;
  onStop: () => Promise<void> | void;
  onQuit: (dontShowAgain: boolean) => void;
  onCancel: () => void;
}) {
  const answers = useRef<HTMLDivElement>(null);
  const [dontShowAgain, setDontShowAgain] = useState(false);
  const [stopState, setStopState] = useState<"idle" | "running" | "done" | "failed">("idle");
  const host = deviceName("this", os);
  const routedHere = prompt.mode === "ours";
  const canStop = routedHere && canAutoSetup(os);
  const optOut = prompt.mode !== "ours";

  // onStop runs synchronously on the click; only its rejection is awaited, so
  // a handler that throws outright is caught the same as one that rejects. The
  // button is disabled for the whole in-flight window with a progress line above
  // it (fork D4), since unset_proxy is elevation-gated on macOS and slow enough
  // to invite a second click that would fire a second prompt.
  const attemptStop = () => {
    // The button is about to be disabled; a browser blurs a focused control when
    // that happens, dropping focus outside the panel where Tab escapes the modal.
    // Hand focus to quit before it goes.
    const row = answers.current;
    if (row?.contains(document.activeElement)) {
      row.querySelector<HTMLElement>('[data-exit="true"] button')?.focus();
    }
    setStopState("running");
    try {
      Promise.resolve(onStop()).then(
        () => setStopState("done"),
        () => setStopState("failed"),
      );
    } catch {
      setStopState("failed");
    }
  };

  const steps = routedHere || prompt.mode === "unread" ? (
    <ManualFallback
      summary={CLOSE_COPY.byHand}
      collapsed={canStop}
      defaultOpen={stopState === "failed"}
      testId="proxy-close-manual"
      className="mb-3"
    >
      <div data-testid="proxy-close-steps">
        <GuideSteps steps={routeUndo("this", os)} />
      </div>
    </ManualFallback>
  ) : (
    <div className="mb-3">
      <DeviceSteps os={os} initial={deviceShown(prompt, chosen)} />
    </div>
  );

  return (
    <ModalShell title={titleOf(prompt, host)} onCancel={onCancel} testId="proxy-close" mode={prompt.mode}>
      <p className="mb-3 font-ui text-body text-ink-mute">{stakeOf(prompt, host)}</p>
      {stopState === "running" && (
        <div className="mb-2" data-testid="proxy-close-running">
          <Notice tone="pending">{CLOSE_COPY.stopping(os)}</Notice>
        </div>
      )}
      {stopState === "done" && (
        <div className="mb-2" data-testid="proxy-close-done">
          <Notice tone="ok">{CLOSE_COPY.stopped(host)}</Notice>
        </div>
      )}
      {stopState === "failed" && (
        <div className="mb-2" data-testid="proxy-close-failed">
          <Notice tone="warn">{CLOSE_COPY.failed}</Notice>
        </div>
      )}
      {steps}
      {optOut && (
        <div className="mb-3">
          <Checkbox checked={dontShowAgain} onChange={setDontShowAgain}>
            {CLOSE_COPY.dontShowAgain}
          </Checkbox>
        </div>
      )}
      <div ref={answers} className="flex justify-end gap-chrome-gap">
        <span data-exit="true" data-opener={canStop && stopState !== "done" ? undefined : "true"}>
          <Button onClick={() => onQuit(optOut && dontShowAgain)} frame="bare" tone="mute">
            {CLOSE_COPY.quit}
          </Button>
        </span>
        {canStop && stopState !== "done" && (
          <span data-opener="true">
            <Button onClick={attemptStop} disabled={stopState === "running"}>
              {stopState === "failed" ? CLOSE_COPY.tryAgain : CLOSE_COPY.stop}
            </Button>
          </span>
        )}
      </div>
    </ModalShell>
  );
}
