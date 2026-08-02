import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { proxyStatus, trustStatus, type StepStatusView } from "../setup";
import { StatusLine } from "./StatusLine";

const glyphClass = (status: StepStatusView) => {
  const { container } = render(<StatusLine status={status} />);
  return container.querySelector("span[aria-hidden] > span")?.className ?? "";
};

describe("StatusLine", () => {
  it.each([
    ["✓", "ok", "text-ok"],
    ["⚠", "warn", "text-warn"],
    ["◐", "muted", "text-ink-mute"],
    ["○", "muted", "text-ink-mute"],
  ] as const)("%s renders in the %s tone", (glyph, tone, token) => {
    expect(glyphClass({ glyph, tone, message: "a reason" })).toContain(token);
  });

  it.each(["✓", "⚠", "◐", "○"])("%s is a second channel, so tone is never the only one", (glyph) => {
    render(<StatusLine status={{ glyph, tone: "muted", message: "a reason" }} />);
    expect(screen.getByText(glyph)).toBeInTheDocument();
  });

  it("in-flight draws a spinner instead of a glyph — an animation, not a verdict", () => {
    render(<StatusLine status={{ glyph: "⟳", tone: "muted", message: "Installing…" }} />);

    expect(screen.getByTestId("status-line-spinner").className).toContain("animate-spin");
    expect(screen.queryByText("⟳")).not.toBeInTheDocument();
  });

  it("renders the optional name in body ink, never toned", () => {
    render(
      <StatusLine status={{ glyph: "⚠", tone: "warn", message: "Install failed" }} name="Certificate" />,
    );

    const name = screen.getByText("Certificate");
    expect(name.className).toContain("text-ink-body");
    expect(name.className).not.toContain("text-warn");
  });

  it("renders without a name — the name is optional, not a placeholder", () => {
    render(<StatusLine status={{ glyph: "○", tone: "muted", message: "Not trusted yet" }} />);
    expect(screen.getByText("Not trusted yet")).toBeInTheDocument();
  });

  it("wraps the message rather than truncating it — a cut reason is a wrong reason", () => {
    const message =
      "Traffic is arriving but not decrypting — install the certificate on the device you're capturing from.";
    render(<StatusLine status={{ glyph: "⚠", tone: "warn", message }} />);

    const rendered = screen.getByText(message);
    expect(rendered.className).not.toContain("truncate");
    expect(rendered.className).not.toContain("line-clamp");
    expect(rendered.className).toContain("min-w-0");
  });

  it("is pure display: no checkbox, no toggle, nothing focusable", () => {
    const { container } = render(
      <StatusLine status={{ glyph: "✓", tone: "ok", message: "Trusted" }} name="Certificate" />,
    );

    expect(container.querySelector("input, button, [role=checkbox], [tabindex]")).toBeNull();
  });

  // The precedence is the contract this component renders, and the reason it is
  // T2. Driven through the real resolvers so a re-ranking in either place reds.
  describe("the resolved verdict it is handed", () => {
    const seen = { decrypted: false, trafficSeen: false };

    it("shows an authoritative positive over a stale action error", () => {
      render(<StatusLine status={trustStatus("trusted", seen, null, "install exploded")} />);

      expect(screen.getByText("✓")).toBeInTheDocument();
      expect(screen.queryByText(/install exploded/)).not.toBeInTheDocument();
    });

    it("shows a running action over every settled verdict, error included", () => {
      render(<StatusLine status={trustStatus("untrusted", seen, "installing", "boom")} />);

      expect(screen.getByTestId("status-line-spinner")).toBeInTheDocument();
      expect(screen.getByText("Installing…")).toBeInTheDocument();
    });

    it("shows observed proof over a stale error when the probe cannot say", () => {
      render(
        <StatusLine
          status={proxyStatus("unknown", { decrypted: true, trafficSeen: true }, null, "set failed")}
        />,
      );

      expect(screen.getByText("Receiving traffic")).toBeInTheDocument();
      expect(glyphClass(proxyStatus("unknown", { decrypted: true, trafficSeen: true }, null, "set failed"))).toContain(
        "text-ok",
      );
    });

    it("shows the error when nothing positive outranks it", () => {
      render(<StatusLine status={proxyStatus("unknown", seen, null, "port busy")} />);

      expect(screen.getByText(/port busy/)).toBeInTheDocument();
      expect(screen.getByText("⚠")).toBeInTheDocument();
    });
  });

  it("composes TonedText — the glyph's tone comes from the primitive", () => {
    render(<StatusLine status={{ glyph: "✓", tone: "ok", message: "Trusted" }} />);

    const glyph = screen.getByText("✓");
    expect(glyph.className).toContain("font-mono");
    expect(glyph.className).toContain("text-ok");
  });
});
