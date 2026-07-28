import statuses from "statuses";
import { statusTone } from "../flow";
import { TonedText } from "./TonedText";

/**
 * StatusPill — HTTP status value tinted by outcome tone; the sanctioned chromatic exception in Warp's zero-accent scheme.
 * @tier T2 — owns the status→tone mapping (2xx=ok, 4xx/5xx=warn, else muted) and the reason-phrase alt text; status in, full display out
 * @anatomy TonedText carrying the status code ("…" when null); title + aria-label = "<code> <reason phrase>" via the statuses lib
 * @use anywhere a flow's HTTP outcome is shown — pass the raw status (number | null), nothing else to resolve or format
 * @avoid pre-resolving tone or formatting at the call site — that is this component's job · a toned value that is not an HTTP status — use TonedText (T1) · proxy connection health — StatusDot
 * @states tone:ok (2xx) · tone:warn (4xx/5xx) · tone:muted (3xx, 1xx, unknown, null/pending → "…") · no interaction states (pure display)
 * @madeof TonedText
 */
export function StatusPill({ status }: { status: number | null }) {
  const phrase = status === null ? "pending" : `${status} ${statuses.message[status] ?? "unknown"}`;

  return (
    <span title={phrase} aria-label={phrase}>
      <TonedText tone={statusTone(status)}>{status ?? "…"}</TonedText>
    </span>
  );
}
