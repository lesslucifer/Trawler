import { apiAddr } from "./api";
import { Button, Notice } from "./components/index";

// One sentence for one cause, mounted in two places: the shell band while rows
// are on screen, the row region when there are none. The trailing clause is what
// differs, because what the silence costs differs — stale rows or no rows at all.
//
// Fork D4 is why `retrying` drives both the disabled button and the detail line:
// disabling a button without saying what is awaited is half a contract.
export function NotAnswering({
  trailing,
  retrying,
  onRetry,
}: {
  trailing: string;
  retrying: boolean;
  onRetry: () => void;
}) {
  const addr = apiAddr();
  return (
    <Notice
      tone="warn"
      detail={retrying ? `trying ${addr}…` : `no answer from ${addr} — ${trailing}`}
      action={
        <Button onClick={onRetry} disabled={retrying}>
          retry
        </Button>
      }
    >
      trawler is not answering
    </Notice>
  );
}
