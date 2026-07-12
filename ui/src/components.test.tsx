import { render, screen } from "@testing-library/react";
import { Field, StatusPill, LiveDot } from "./components";

test("Field renders with Warp theme token classes", () => {
  render(<Field aria-label="filter" />);
  const input = screen.getByLabelText("filter");
  expect(input.className).toMatch(/bg-canvas/);
  expect(input.className).toMatch(/border-hairline/);
});

test("StatusPill maps tone to a functional status color class", () => {
  const { rerender } = render(<StatusPill tone="ok">200</StatusPill>);
  expect(screen.getByText("200").className).toMatch(/text-ok/);
  rerender(<StatusPill tone="warn">500</StatusPill>);
  expect(screen.getByText("500").className).toMatch(/text-warn/);
});

test("LiveDot reflects connection state", () => {
  const { rerender } = render(<LiveDot live={false} />);
  expect(screen.getByTestId("live").className).toMatch(/bg-warn/);
  rerender(<LiveDot live />);
  expect(screen.getByTestId("live").className).toMatch(/bg-ok/);
});
