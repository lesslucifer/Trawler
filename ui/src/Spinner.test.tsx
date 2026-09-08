import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { Spinner } from "./Spinner";

test("the ring turns, and says nothing a reader has not already been told", () => {
  render(<Spinner />);

  const ring = screen.getByTestId("spinner");
  expect(ring).toHaveAttribute("aria-hidden", "true");
  expect(ring.className).toContain("animate-spin");
  // Round, hairline, one arc inked: the turn is only visible because a single
  // side differs from the other three.
  expect(ring.className).toContain("rounded-full");
  expect(ring.className).toContain("border-t-ink-mute");
});
