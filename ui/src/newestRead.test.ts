import { describe, expect, it } from "vitest";
import { newestRead } from "./newestRead";

describe("a read that only lands while it is the newest", () => {
  it("lands the newest read, and not one a later read has overtaken", () => {
    const reads = newestRead();
    const first = reads.begin("shown")!;
    const second = reads.begin("shown")!;

    expect(first()).toBe(false);
    expect(second()).toBe(true);
  });

  it("asked for by nobody, waits for the one in flight rather than overtaking it", () => {
    const reads = newestRead();
    const asked = reads.begin("shown")!;

    expect(reads.begin("quiet")).toBeNull();
    expect(asked()).toBe(true);
    expect(reads.begin("quiet")).not.toBeNull();
  });

  it("asked for, overtakes the one in flight", () => {
    const reads = newestRead();
    const quiet = reads.begin("quiet")!;
    const asked = reads.begin("shown")!;

    expect(quiet()).toBe(false);
    expect(asked()).toBe(true);
  });

  it("is overtaken by an answer the caller holds of its own, which leaves nothing in flight", () => {
    const reads = newestRead();
    const inFlight = reads.begin("quiet")!;
    reads.supersede();

    expect(inFlight()).toBe(false);
    expect(reads.begin("quiet")).not.toBeNull();
  });
});
