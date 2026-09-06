import { bench, describe } from "vitest";
import { merge } from "../src/useFlows";
import { flows } from "./fixtures";

const LIST = flows(5000);

// Every refresh after the first is the steady state: the list is re-fetched
// whole, and it holds no host the accumulated set has not already seen.
const KNOWN = merge([], LIST);

describe("useFlows.merge — host accumulation per debounced refresh", () => {
  bench("steady state — 5000 flows, every host already known", () => {
    merge(KNOWN, LIST);
  });

  bench("cold — 5000 flows into an empty set", () => {
    merge([], LIST);
  });
});
