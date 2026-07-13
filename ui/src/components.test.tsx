import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { Field, StatusPill, StatusDot } from "./components";

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

test("StatusDot maps capture status to a color and is tappable to re-run setup", () => {
  const onClick = vi.fn();
  const { rerender } = render(<StatusDot status="broken" onClick={onClick} />);
  const dot = screen.getByTestId("status-dot");
  expect(dot.className).toMatch(/bg-warn/);
  fireEvent.click(dot);
  expect(onClick).toHaveBeenCalledOnce();
  rerender(<StatusDot status="checking" onClick={onClick} />);
  expect(screen.getByTestId("status-dot").className).toMatch(/bg-ink-mute/);
  rerender(<StatusDot status="connected" onClick={onClick} />);
  expect(screen.getByTestId("status-dot").className).toMatch(/bg-ok/);
});
