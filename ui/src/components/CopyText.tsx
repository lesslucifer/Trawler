import { useConfirm } from "./useConfirm";

/**
 * CopyText — a displayed value the user grabs whole; click copies it verbatim.
 * @tier T1 — display + click-copy, no business logic
 * @anatomy value verbatim (mono, wraps, never truncates) · dotted underline, I-beam cursor kept (un-link-like on purpose) · transient outcome chip beside it — "copied" in ok, "copy failed" in warn
 * @use full-value fields grabbed whole: the request URI, a flow id, every KeyValueRow value
 * @avoid prose · showing anything but the exact payload · explicit copy actions with their own label (that is CopyButton)
 * @states rest · hover · focus · confirming (~2 s, resets on value change) · error ("copy failed", warn tone, same timer rules — a rejected write is never silent) · drag-select passes through to native selection
 * @madeof — (shares the confirm convention with CopyButton via one hook)
 */
export function CopyText({
  value,
  label,
  className = "",
}: {
  value: string;
  label: string;
  className?: string;
}) {
  const { state, flash } = useConfirm(value);
  const confirming = state === "confirming";
  const failed = state === "error";

  const copy = () => {
    if (!window.getSelection()?.isCollapsed) return;
    Promise.resolve(navigator.clipboard?.writeText(value)).then(
      () => flash("confirming"),
      () => flash("error"),
    );
  };

  return (
    <span className={className}>
      <span
        role="button"
        tabIndex={0}
        aria-label={`copy ${label}`}
        title="click to copy"
        onClick={copy}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            copy();
          }
        }}
        className={`cursor-text break-all font-mono text-body underline decoration-dotted underline-offset-link hover:text-ink focus-visible:outline focus-visible:outline-focus focus-visible:outline-ink-mute focus-visible:outline-offset-focus ${
          confirming
            ? "decoration-ok text-ok"
            : failed
              ? "decoration-warn text-warn"
              : "text-ink-body decoration-hairline hover:decoration-ink-mute"
        }`}
      >
        {value}
      </span>
      {(confirming || failed) && (
        <span
          className={`ml-control-gap rounded-button border px-chrome-x font-ui text-label ${
            confirming ? "border-ok text-ok" : "border-warn text-warn"
          }`}
        >
          {confirming ? "copied" : "copy failed"}
        </span>
      )}
    </span>
  );
}
