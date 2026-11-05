import { useCopy } from "./useConfirm";

/**
 * CopyText — a displayed value the user grabs whole; click copies it verbatim.
 * @tier T1 — display + click-copy, no business logic
 * @anatomy value verbatim (mono, wraps, never truncates) · dotted underline, I-beam cursor kept (un-link-like on purpose) · transient outcome chip beside it — "copied" in ok, "copy failed" in warn · oneLine: the value on one line, cut at its box's end with an ellipsis (DEC-78); the chip stays whole at the end and the value shortens for it; a click still copies the whole value
 * @use full-value fields grabbed whole: a flow id, a host, a port · oneLine wherever a data window holds the whole value: the request URI, every KeyValueRow value
 * @avoid prose · showing anything but the exact payload · explicit copy actions with their own label (that is CopyButton) · oneLine on a value with no data-window route to its whole (DEC-78 rule 5)
 * @states rest · cut (oneLine, an ellipsis shows) · hover · focus · confirming (~2 s, resets on value change) · error ("copy failed", warn tone, same timer rules — a rejected write is never silent) · drag-select passes through to native selection
 * @madeof — (shares the confirm convention with CopyButton via one hook)
 */
export function CopyText({
  value,
  label,
  oneLine = false,
  className = "",
}: {
  value: string;
  label: string;
  oneLine?: boolean;
  className?: string;
}) {
  const { state, copy: write } = useCopy(value);
  const confirming = state === "confirming";
  const failed = state === "error";

  const copy = () => {
    if (!window.getSelection()?.isCollapsed) return;
    write(value);
  };

  return (
    <span className={oneLine ? `flex min-w-0 items-center ${className}` : className}>
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
        className={`cursor-text ${oneLine ? "min-w-0 truncate" : "break-all"} font-mono text-body underline decoration-dotted underline-offset-link hover:text-ink focus-visible:outline focus-visible:outline-focus focus-visible:outline-ink-mute focus-visible:outline-offset-focus ${
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
          className={`ml-control-gap ${oneLine ? "shrink-0 " : ""}rounded-button border px-chrome-x font-ui text-label ${
            confirming ? "border-ok text-ok" : "border-warn text-warn"
          }`}
        >
          {confirming ? "copied" : "copy failed"}
        </span>
      )}
    </span>
  );
}
