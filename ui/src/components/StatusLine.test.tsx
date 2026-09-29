import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { NO_TRAFFIC, proxyStatus, trafficStatus, trustStatus, type StepStatusView } from "../verdicts";
import { StatusLine } from "./StatusLine";

const CONFIRM_ON = "this Mac's first request confirms it";

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
    expect(glyphClass({ glyph, tone, short: "a reason" })).toContain(token);
  });

  it.each(["✓", "⚠", "◐", "○"])("%s is a second channel, so tone is never the only one", (glyph) => {
    render(<StatusLine status={{ glyph, tone: "muted", short: "a reason" }} />);
    expect(screen.getByText(glyph)).toBeInTheDocument();
  });

  it("in-flight draws a spinner instead of a glyph — an animation, not a verdict", () => {
    render(
      <StatusLine status={{ glyph: "⟳", tone: "muted", short: "installing…" }} />,
    );

    expect(screen.getByTestId("status-line-spinner").className).toContain("animate-spin");
    expect(screen.queryByText("⟳")).not.toBeInTheDocument();
  });

  it("renders the optional name in body ink, never toned", () => {
    render(
      <StatusLine
        status={{ glyph: "⚠", tone: "warn", short: "install failed" }}
        name="Certificate"
      />,
    );

    const name = screen.getByText("Certificate");
    expect(name.className).toContain("text-ink-body");
    expect(name.className).not.toContain("text-warn");
  });

  it("renders without a name — the name is optional, not a placeholder", () => {
    render(
      <StatusLine
        status={{ glyph: "○", tone: "muted", short: "not trusted yet" }}
      />,
    );
    expect(screen.getByText("not trusted yet")).toBeInTheDocument();
  });

  it("wraps the message rather than truncating it — a cut reason is a wrong reason", () => {
    const short = "not working — arriving but not decrypting";
    render(
      <StatusLine status={{ glyph: "⚠", tone: "warn", short }} />,
    );

    const rendered = screen.getByText(short);
    expect(rendered.className).not.toContain("truncate");
    expect(rendered.className).not.toContain("line-clamp");
    expect(rendered.className).toContain("min-w-0");
  });

  it.each([
    ["ok", "text-ok"],
    ["warn", "text-warn"],
    ["muted", "text-ink-mute"],
  ] as const)("the %s verdict's words carry the tone, not just its glyph", (tone, token) => {
    render(<StatusLine status={{ glyph: "✓", tone, short: "the verdict" }} />);

    const rendered = screen.getByText("the verdict");
    expect(rendered.className).toContain(token);
    expect(rendered.className).toContain("font-ui");
  });

  it("renders short, never the remedy — the call site does not shorten either", () => {
    const status = trafficStatus({ ...NO_TRAFFIC, lastPlainAt: 9_000 }, 10_000);
    render(<StatusLine status={status} />);

    expect(screen.getByText("not done — only plain http so far")).toBeInTheDocument();
    expect(screen.queryByText(/starts with https/)).not.toBeInTheDocument();
  });

  it("leaves the remedy to Notice — a verdict and an instruction are two elements", () => {
    const status = trafficStatus({ ...NO_TRAFFIC, lastTunnelAt: 0 }, 10_000);
    render(<StatusLine status={status} />);

    expect(status.remedy).toMatch(/does not trust Trawler's certificate yet/);
    expect(screen.queryByText(status.remedy!)).not.toBeInTheDocument();
  });

  it("is pure display: no checkbox, no toggle, nothing focusable", () => {
    const { container } = render(
      <StatusLine
        status={{ glyph: "✓", tone: "ok", short: "trusted" }}
        name="Certificate"
      />,
    );

    expect(container.querySelector("input, button, [role=checkbox], [tabindex]")).toBeNull();
  });

  // The precedence is the contract this component renders, and the reason it is
  // T2. Driven through the real resolvers so a re-ranking in either place reds.
  describe("the resolved verdict it is handed", () => {
    it("shows an authoritative positive as the resolved verdict it is", () => {
      render(<StatusLine status={trustStatus("trusted", null, CONFIRM_ON)} />);

      expect(screen.getByText("✓")).toBeInTheDocument();
      expect(screen.getByText("done — installed and trusted")).toBeInTheDocument();
    });

    it("shows a running action over every settled verdict", () => {
      render(<StatusLine status={trustStatus("trusted", "installing", CONFIRM_ON)} />);

      expect(screen.getByTestId("status-line-spinner")).toBeInTheDocument();
      expect(screen.getByText("setting up")).toBeInTheDocument();
    });

    it("shows a decrypted flow arriving now as the ok verdict of the traffic layer", () => {
      const status = trafficStatus({ ...NO_TRAFFIC, lastDecryptedAt: 9_000 }, 10_000);
      render(<StatusLine status={status} />);

      expect(screen.getByText("done — capturing traffic")).toBeInTheDocument();
      expect(glyphClass(status)).toContain("text-ok");
    });

    it("shows the can't-determine glyph where nothing read the state at all", () => {
      render(
        <StatusLine
          status={proxyStatus("unknown", null, { device: "this Mac", confirmOn: CONFIRM_ON })}
        />,
      );

      expect(screen.getByText(`can't check here — ${CONFIRM_ON}`)).toBeInTheDocument();
      expect(screen.getByText("◐")).toBeInTheDocument();
    });
  });

  it("composes TonedText — the glyph's tone comes from the primitive", () => {
    render(<StatusLine status={{ glyph: "✓", tone: "ok", short: "trusted" }} />);

    const glyph = screen.getByText("✓");
    expect(glyph.className).toContain("font-mono");
    expect(glyph.className).toContain("text-ok");
  });
});
