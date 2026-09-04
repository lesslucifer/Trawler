import { COPY, type DotStatus } from "../onboardingMachine";
import { Button } from "./Button";

const FILL: Record<DotStatus, string> = {
  broken: "bg-warn",
  checking: "bg-ink-mute",
  connected: "bg-ok",
};

/**
 * StatusDot — capture health as one disc, and the app's only way back into setup.
 * @tier T2 — owns the DotStatus→fill map (broken=warn, checking=ink-mute, connected=ok) and the DotStatus→phrase map that gives every state its words; status in, full display out. The phrases come from COPY.dotTitle, the status from dotStatus() — it derives neither
 * @anatomy a 24×24 bare-square button (Button's frame="bare" shape="square" shell — the app's hit floor, WCAG 2.5.8) wrapping a 10px rounded-full disc (spinner token) whose background carries the state · the disc is aria-hidden; the button's aria-label and title both carry the verdict in words · data-status carries the raw state for tests
 * @use the app header's capture indicator — the single tenant, on screen in both the onboarding and proven phases. Pass a resolved DotStatus and the handler that re-invokes setup
 * @avoid an HTTP status (use StatusPill) · a precondition's verdict with a name and message (use StatusLine) · a toned value that is text (use TonedText — this renders a shape, and its colour is a background, not a color) · deriving the status from app phase inside the component (dotStatus() owns that, DEC-20) · a caller-supplied phrase, tone or size — each is the component's, per state · disabling it in any state: it is the only route back to setup, so a disabled dot is a dead end · rendering it as a bare span or a ● glyph, which loses the hit target, the accessible name, or both
 * @states broken (bg-warn — setup incomplete or capture never proven) · checking (bg-ink-mute — the launch self-test is still resolving; a lifecycle transient, NOT StatusTone's terminal muted) · connected (bg-ok — a decrypted HTTPS flow has been seen) · hover (wash behind the disc; the fill never changes) · focus (the shared 1px ink-mute outline). No disabled, loading, error or fourth state, and no pulse on checking — motion would carry nothing the fill does not
 * @madeof Button (T1) with frame="bare" shape="square" — element, focus ring, radius, hit floor and keyboard come from the shell, the way IconButton composes it. NOT TonedText, unlike its two status siblings: there is no text to tone
 */
export function StatusDot({ status, onClick }: { status: DotStatus; onClick: () => void }) {
  const phrase = COPY.dotTitle[status];

  return (
    <Button onClick={onClick} frame="bare" shape="square" label={phrase} title={phrase}>
      <span
        aria-hidden="true"
        data-testid="status-dot"
        data-status={status}
        className={`inline-block h-spinner w-spinner rounded-full ${FILL[status]}`}
      />
    </Button>
  );
}
