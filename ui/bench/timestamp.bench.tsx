import { bench, describe } from "vitest";
import { render } from "@testing-library/react";
import { TimestampText } from "../src/components/TimestampText";
import { flows } from "./fixtures";

// The virtualizer draws a ~30-row window with 12 rows of overscan, and the whole
// window re-renders on every debounced SSE refresh — so the subject is a
// re-render of that window, not a mount.
const WINDOW = 42;

const ROWS = flows(WINDOW);

function TimeColumn({ generation }: { generation: number }) {
  return (
    <div data-generation={generation}>
      {ROWS.map((flow) => (
        <TimestampText key={flow.id} timestamp={flow.started_at} />
      ))}
    </div>
  );
}

describe("TimestampText — the flow list's time column", () => {
  const view = render(<TimeColumn generation={0} />);
  let generation = 0;

  bench(`re-render of a ${WINDOW}-row window`, () => {
    generation += 1;
    view.rerender(<TimeColumn generation={generation} />);
  });

  bench(`first mount of a ${WINDOW}-row window`, () => {
    render(<TimeColumn generation={0} />).unmount();
  });
});
