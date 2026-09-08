import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { Spinner } from "./Spinner";

const ring = () => screen.getByTestId("spinner");

test("it turns, which is the whole component", () => {
  render(<Spinner />);
  expect(ring().className).toContain("animate-spin");
});

// Round, hairline all round, one edge inked: the turn is only legible because a
// single arc differs from the other three. Drop any of the three and the ring
// either stops reading as a ring or stops reading as turning.
test("one arc is inked darker than the other three, inside a full circle", () => {
  render(<Spinner />);
  expect(ring().className).toContain("rounded-full");
  expect(ring().className).toContain("border-hairline");
  expect(ring().className).toContain("border-t-ink-mute");
});

// The deliberate omission, pinned so a later "accessibility fix" has to argue
// with this test rather than quietly add a name: every sanctioned use puts the
// ring beside a line that already says what is happening in words.
test("it says nothing to a screen reader, because its neighbour already does", () => {
  render(<Spinner />);
  expect(ring()).toHaveAttribute("aria-hidden", "true");
  expect(ring()).not.toHaveAccessibleName();
  expect(ring().textContent).toBe("");
});

// A flex row with a long in-flight line must not squeeze it into an ellipse.
test("it holds its size and its round in a flex row", () => {
  render(<Spinner />);
  expect(ring().className).toContain("shrink-0");
  expect(ring().className).toContain("h-spinner");
  expect(ring().className).toContain("w-spinner");
});
