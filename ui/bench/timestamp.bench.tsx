import { bench, describe } from "vitest";
import { render } from "@testing-library/react";
import type { FlowSummary } from "../src/api";
import { TimestampText } from "../src/components/TimestampText";
import { flows } from "./fixtures";

// The virtualizer draws a ~30-row window with 12 rows of overscan, and the whole
// window re-renders on every debounced SSE refresh — so the subject is a
// re-render of that window, not a mount.
const WINDOW = 42;

const ROWS = flows(WINDOW);

// A flow's started_at never changes, so re-rendering a window that has not
// scrolled formats timestamps already seen. A live-tailing capture is the other
// half: new flows arrive, and the window they land in holds timestamps formatted
// for the first time. One case each, because a memo priced only on hits prices
// only half the workload — the pool is sized past the iteration count so a
// measured iteration can never wrap back onto a warm one.
const MISS_ITERATIONS = 400;

const UNSEEN = flows(WINDOW * (MISS_ITERATIONS + 25));

function TimeColumn({ rows }: { rows: FlowSummary[] }) {
  return (
    <div>
      {rows.map((flow) => (
        <TimestampText key={flow.id} timestamp={flow.started_at} />
      ))}
    </div>
  );
}

describe("TimestampText — the flow list's time column", () => {
  const view = render(<TimeColumn rows={ROWS} />);

  bench(`re-render of a ${WINDOW}-row window, every timestamp seen before`, () => {
    view.rerender(<TimeColumn rows={ROWS} />);
  });

  let unseenAt = 0;
  bench(
    `re-render of a ${WINDOW}-row window, every timestamp new`,
    () => {
      const rows = UNSEEN.slice(unseenAt, unseenAt + WINDOW);
      unseenAt += WINDOW;
      view.rerender(<TimeColumn rows={rows} />);
    },
    { iterations: MISS_ITERATIONS, time: 0, warmupIterations: 10, warmupTime: 0 },
  );

  bench(`first mount of a ${WINDOW}-row window`, () => {
    render(<TimeColumn rows={ROWS} />).unmount();
  });
});
