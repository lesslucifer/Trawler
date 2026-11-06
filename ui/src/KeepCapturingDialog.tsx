import { Button, Notice } from "./components/index";
import { ModalShell } from "./ModalShell";
import { GNOME_TRAY_HINT, trayPlace } from "./generalPrefs";
import { hostOs, type HostOs } from "./setup";

export const KEEP_COPY = {
  title: "keep capturing after the window closes?",
  stays: (os: HostOs) =>
    `Trawler stays in ${trayPlace(os)} and keeps capturing${os === "linux" ? ` — ${GNOME_TRAY_HINT} it` : ""}. You can change this in Settings → General.`,
  quit: "quit",
  keep: "keep capturing",
} as const;

export function KeepCapturingDialog({
  onKeep,
  onQuit,
  onCancel,
  refused = null,
  os = hostOs(),
}: {
  onKeep: () => void;
  onQuit: () => void;
  onCancel: () => void;
  refused?: string | null;
  os?: HostOs;
}) {
  return (
    <ModalShell title={KEEP_COPY.title} onCancel={onCancel} testId="keep-capturing">
      <p className="mb-3 font-ui text-body text-ink-mute">{KEEP_COPY.stays(os)}</p>
      {refused !== null && (
        <div className="mb-3">
          <Notice tone="warn">{`couldn't save — ${refused}`}</Notice>
        </div>
      )}
      <div className="flex justify-end gap-chrome-gap">
        <Button onClick={onQuit} frame="bare" tone="mute">
          {KEEP_COPY.quit}
        </Button>
        <span data-opener="true">
          <Button onClick={onKeep}>{KEEP_COPY.keep}</Button>
        </span>
      </div>
    </ModalShell>
  );
}
