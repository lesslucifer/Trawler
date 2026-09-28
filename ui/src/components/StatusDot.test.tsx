import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { COPY, type DotOpens, type DotStatus } from "../onboardingMachine";
import { StatusDot } from "./StatusDot";

const STATES: DotStatus[] = ["broken", "checking", "quiet", "connected"];
const name = (status: DotStatus, opens: DotOpens = "setup") =>
  `${COPY.dotTitle[status]} — ${COPY.dotAction[opens]}`;

describe("StatusDot", () => {
  it("maps each state to its fill and derives nothing from app state (B1)", () => {
    const fills: Record<DotStatus, string> = {
      broken: "bg-warn",
      checking: "bg-ink-mute",
      quiet: "border-ok",
      connected: "bg-ok",
    };

    for (const status of STATES) {
      const { unmount } = render(<StatusDot status={status} opens="setup" onClick={() => {}} />);
      expect(screen.getByTestId("status-dot").className).toContain(fills[status]);
      unmount();
    }
  });

  it("every state carries its verdict in words, so it never announces as a bare button (B2, S4)", () => {
    for (const status of STATES) {
      const { unmount } = render(<StatusDot status={status} opens="setup" onClick={() => {}} />);

      const button = screen.getByRole("button", { name: name(status) });
      expect(button).toHaveAttribute("title", name(status));
      unmount();
    }
  });

  it("quiet keeps ok's hue as a ring and loses the fill, so it reads apart from capturing without colour alone", () => {
    render(<StatusDot status="quiet" opens="setup" onClick={() => {}} />);

    const disc = screen.getByTestId("status-dot");
    expect(disc.className).toContain("border-emphasis");
    expect(disc.className).toContain("border-ok");
    expect(disc.className).not.toContain("bg-");
    expect(screen.getByRole("button", { name: "Nothing captured in the last 3 minutes — click to set up" })).toBeVisible();
  });

  it("says where a click goes: setup from the list, the list from setup", () => {
    const { rerender } = render(<StatusDot status="connected" opens="setup" onClick={() => {}} />);
    expect(screen.getByRole("button", { name: "Capturing — click to set up" })).toBeVisible();

    rerender(<StatusDot status="connected" opens="list" onClick={() => {}} />);
    expect(screen.getByRole("button", { name: "Capturing — click to show traffic" })).toBeVisible();
  });

  it("the disc is aria-hidden, so the verdict is announced once and not twice", () => {
    render(<StatusDot status="broken" opens="setup" onClick={() => {}} />);

    const disc = screen.getByTestId("status-dot");
    expect(disc).toHaveAttribute("aria-hidden", "true");
    expect(disc).toHaveAttribute("data-status", "broken");
    expect(disc).toHaveTextContent("");
  });

  it("the hit target is the 24px button, not the 10px disc (S3 — the shipped defect)", () => {
    render(<StatusDot status="connected" opens="setup" onClick={() => {}} />);

    const button = screen.getByRole("button", { name: name("connected") });
    expect(button.style.width).toBe("24px");
    expect(button.style.height).toBe("24px");

    const disc = screen.getByTestId("status-dot");
    expect(disc.className).toContain("h-spinner");
    expect(disc.className).toContain("w-spinner");
    expect(disc.tagName).not.toBe("BUTTON");
  });

  it("is clickable in every state, the good one included (B3)", () => {
    for (const status of STATES) {
      const onClick = vi.fn();
      const { unmount } = render(<StatusDot status={status} opens="setup" onClick={onClick} />);

      const button = screen.getByRole("button", { name: name(status) });
      expect(button).not.toBeDisabled();
      fireEvent.click(button);
      expect(onClick).toHaveBeenCalledOnce();
      unmount();
    }
  });

  it("hovers as a wash behind the disc; the fill itself never changes", () => {
    render(<StatusDot status="connected" opens="setup" onClick={() => {}} />);

    const button = screen.getByRole("button", { name: name("connected") });
    expect(button.className).toContain("hover:bg-wash-hover");
    expect(screen.getByTestId("status-dot").className).not.toContain("hover:");
  });

  it("neither pulses nor spins on checking — motion would carry nothing the fill does not", () => {
    render(<StatusDot status="checking" opens="setup" onClick={() => {}} />);

    const disc = screen.getByTestId("status-dot");
    expect(disc.className).not.toContain("animate");
  });
});
