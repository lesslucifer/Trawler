import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { CopyText } from "./CopyText";
import { StepList } from "./StepList";

describe("StepList", () => {
  it("is a real ordered list, one item per step, in array order", () => {
    const { container } = render(
      <StepList steps={["open settings", "tap the profile", "trust it"]} />,
    );

    expect(container.querySelector("ol")).toBeInTheDocument();
    const items = screen.getAllByRole("listitem");
    expect(items).toHaveLength(3);
    expect(items[0]).toHaveTextContent("open settings");
    expect(items[2]).toHaveTextContent("trust it");
  });

  it("numbers the markers from one, aria-hidden so the ol carries the order", () => {
    render(<StepList steps={["a", "b"]} />);

    const first = screen.getByText("1.");
    expect(first).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByText("2.")).toBeInTheDocument();
  });

  it("renders an arbitrary node per step — a command step is a CopyText", () => {
    render(<StepList steps={[<CopyText key="c" value="trawler ca install" label="command" />]} />);

    expect(screen.getByRole("button", { name: "copy command" })).toBeInTheDocument();
  });

  it("stacks multiple children inside one step's slot", () => {
    render(
      <StepList
        steps={[
          <>
            <p>run this</p>
            <CopyText value="cmd" label="command" />
          </>,
        ]}
      />,
    );

    const item = screen.getByRole("listitem");
    expect(item).toHaveTextContent("run this");
    expect(item).toHaveTextContent("cmd");
  });

  it("a long step wraps rather than truncating — a cut command is a broken command", () => {
    const long = "trawler ".repeat(50);
    const { container } = render(<StepList steps={[long]} />);

    const body = container.querySelector("li > div");
    expect(body?.className).toContain("min-w-0");
    expect(body?.className).not.toContain("truncate");
    expect(container.querySelector("ol")?.className).not.toContain("overflow");
  });

  it("adds no frame of its own — no box, background, rule or padding", () => {
    const { container } = render(<StepList steps={["a"]} />);

    const list = container.querySelector("ol");
    expect(list?.className).toContain("p-0");
    expect(list?.className ?? "").not.toContain("border");
    expect(list?.className ?? "").not.toContain("bg-");
  });

  it("renders no per-step completion or current-step highlight", () => {
    render(<StepList steps={["a", "b"]} />);

    for (const item of screen.getAllByRole("listitem")) {
      expect(item.className).not.toContain("bg-");
      expect(item).not.toHaveTextContent("✓");
    }
  });

  it("an empty array renders an empty list, not a placeholder", () => {
    render(<StepList steps={[]} />);
    expect(screen.queryAllByRole("listitem")).toHaveLength(0);
  });
});
