import { COPY, type DotOpens, type DotStatus } from "../onboardingMachine";
import { Button } from "./Button";

const FILL: Record<DotStatus, string> = {
  broken: "bg-warn",
  checking: "bg-ink-mute",
  quiet: "border-emphasis border-ok",
  connected: "bg-ok",
};

/**
 * StatusDot — capture recency as one disc, and the switch between the list and setup.
 * @tier T2 — owns the DotStatus→fill map (broken=warn, checking=ink-mute, quiet=an ok ring, connected=ok) and the phrases that give every state and every destination its words; status in, full display out. The phrases come from COPY.dotTitle and COPY.dotAction, the status from dotStatus() and the destination from dotOpens() — it derives neither
 * @anatomy a 24×24 bare-square button (Button's frame="bare" shape="square" shell — the app's hit floor, WCAG 2.5.8) wrapping a 10px rounded-full disc (spinner token) whose background, or for quiet its 1.5px border, carries the state · the disc is aria-hidden; the button's aria-label and title both carry the state and what a click opens, in words ("Capturing — click to set up") · data-status carries the raw state for tests
 * @use the app header's capture indicator and screen switch — the single tenant, on screen in every phase: from the list it opens setup, from setup and the launch check it opens the list (DEC-61). Pass a resolved DotStatus, where the click goes, and the handler that goes there
 * @avoid an HTTP status (use StatusPill) · a precondition's verdict with a name and message (use StatusLine) · a toned value that is text (use TonedText — this renders a shape, and its colour is a background, not a color) · deriving the status from app phase inside the component (dotStatus() owns that, DEC-20) · a caller-supplied phrase, tone or size — each is the component's, per state · disabling it in any state: it is the one control on screen in every phase that switches, so a disabled dot is a dead end · rendering it as a bare span or a ● glyph, which loses the hit target, the accessible name, or both
 * @states broken (bg-warn — nothing captured since the app started) · checking (bg-ink-mute — the core has not answered yet; a lifecycle transient, NOT StatusTone's terminal muted) · quiet (a hollow ok ring — something was captured, nothing in the last 3 minutes: capture worked once and is not shown working now, so it keeps ok's hue and loses its fill) · connected (bg-ok — a request, http or https, captured in the last 3 minutes) · hover (wash behind the disc; the fill never changes) · focus (the shared 1px ink-mute outline). No disabled, loading or error state, and no pulse on checking — motion would carry nothing the fill does not
 * @madeof Button (T1) with frame="bare" shape="square" — element, focus ring, radius, hit floor and keyboard come from the shell, the way IconButton composes it. NOT TonedText, unlike its two status siblings: there is no text to tone
 */
export function StatusDot({
  status,
  opens,
  onClick,
}: {
  status: DotStatus;
  opens: DotOpens;
  onClick: () => void;
}) {
  const phrase = `${COPY.dotTitle[status]} — ${COPY.dotAction[opens]}`;

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
