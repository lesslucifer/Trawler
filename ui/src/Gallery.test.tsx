import { beforeAll, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { Gallery } from "./Gallery";

// ponytail: smoke only — the gallery is a throwaway verification screen, so this
// asserts it mounts rather than re-testing every component's contract.
describe("Gallery", () => {
  // jsdom ships no object-URL support, which ImageRenderer's preview needs.
  beforeAll(() => {
    URL.createObjectURL = vi.fn(() => "blob:gallery");
    URL.revokeObjectURL = vi.fn();
  });

  it("mounts with every section rendered", () => {
    render(<Gallery />);

    expect(screen.getByRole("heading", { name: "Component gallery" })).toBeInTheDocument();
    for (const title of [
      /Button — the only/,
      /CopyButton/,
      /IconButton/,
      /TonedText/,
      /StatusPill/,
      /CopyText/,
      /KeyValueRow/,
      /DurationText/,
      /TimestampText/,
      /SegmentedControl/,
      /Notice/,
      /StepList/,
      /Select/,
      /MethodLabel/,
      /StatusLine/,
      /JsonRenderer/,
      /XmlRenderer/,
      /TextRenderer/,
      /FormRenderer/,
      /HexRenderer/,
      /ImageRenderer/,
      /CutMarker/,
    ]) {
      expect(screen.getByRole("heading", { name: title })).toBeInTheDocument();
    }
  });
});
