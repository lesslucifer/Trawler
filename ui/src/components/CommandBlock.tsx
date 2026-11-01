import { CopyButton } from "./CopyButton";

/**
 * CommandBlock — one command to run, on a surface of its own, with the copy affordance in its corner.
 * @tier T1 — presentation and the copy affordance only; which command, and whether it is worth giving, are the tenant's
 * @anatomy a full-width block on canvas-soft with a hairline border at radius 4 and 12/8 padding, set apart from the prose above it · the command verbatim in DM Mono 12px ink-body, wrapping at spaces, and inside a word only where the word is longer than the line, never truncated or scrolled — a cut command is a broken command, and a word split mid-token reads as two · CopyButton's icon form in the top-right corner, which is the adjacency its @avoid requires, so the label names the command without taking a line
 * @use a command a reader runs somewhere else — a terminal, an elevated shell, a device tool. The setup screen's trust, proxy and push commands, and the quit dialog's proxy-off command
 * @avoid a value a reader types into a field rather than runs (that is CopyText — a host, a port, a URL) · prose with a command inside it: the block is the command, and the sentence introducing it stays outside · more than one command in a block, or a command built at render time from fragments the tenant did not name · a run affordance — nothing here executes anything · output, a prompt character, or a shell that is not the reader's
 * @states rest · the copy affordance's own confirming and error, which CopyButton owns. No hover, focus, empty, loading or disabled state — the block is not interactive and a block with no command is not rendered
 * @madeof CopyButton (T1) — the icon form, in the corner
 */
export function CommandBlock({ value, label }: { value: string; label: string }) {
  return (
    <div className="flex items-start gap-2 rounded-card border border-hairline bg-canvas-soft px-3 py-2">
      <code
        data-wire="shell command"
        className="min-w-0 flex-1 whitespace-pre-wrap break-words font-mono text-body leading-normal text-ink-body"
      >
        {value}
      </code>
      <CopyButton value={value} label={label} iconOnly />
    </div>
  );
}
