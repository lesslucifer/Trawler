import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { Help } from "./Help";

const NOTE = "On Ethernet, change Wi-Fi to Ethernet in the command.";

const renderHelp = () =>
  render(
    <Help topic="on Ethernet" text={NOTE}>
      In Terminal, run:
    </Help>,
  );

describe("Help", () => {
  it("keeps the line it explains, and its text off the page until asked for", () => {
    renderHelp();

    expect(screen.getByText("In Terminal, run:")).toBeInTheDocument();
    expect(screen.queryByText(NOTE)).toBeNull();
    expect(screen.getByRole("button", { name: "help: on Ethernet" })).toHaveAttribute("aria-expanded", "false");
  });

  it("opens its text under the line on a click, and shuts it on the next", () => {
    renderHelp();
    const trigger = screen.getByRole("button", { name: "help: on Ethernet" });

    fireEvent.click(trigger);
    const panel = screen.getByText(NOTE);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(trigger).toHaveAttribute("aria-controls", panel.id);
    expect(panel).toHaveAttribute("data-help");

    fireEvent.click(trigger);
    expect(screen.queryByText(NOTE)).toBeNull();
    expect(trigger).toHaveAttribute("aria-expanded", "false");
  });

  it("is a real button, so Enter and Space reach it as a click does", () => {
    renderHelp();

    const trigger = screen.getByRole("button", { name: "help: on Ethernet" });
    expect(trigger.tagName).toBe("BUTTON");
    expect(trigger).toHaveAttribute("type", "button");
  });

  it("shuts on Esc with focus inside, and puts focus back on the ?", () => {
    renderHelp();
    const trigger = screen.getByRole("button", { name: "help: on Ethernet" });
    fireEvent.click(trigger);

    fireEvent.keyDown(screen.getByText(NOTE), { key: "Escape" });

    expect(screen.queryByText(NOTE)).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("lets Esc pass while shut, for whatever surface holds it", () => {
    let reached = false;
    render(
      <div onKeyDown={() => (reached = true)}>
        <Help topic="on Ethernet" text={NOTE}>
          In Terminal, run:
        </Help>
      </div>,
    );

    fireEvent.keyDown(screen.getByRole("button", { name: "help: on Ethernet" }), { key: "Escape" });

    expect(reached).toBe(true);
  });

  it("shows the ? as IconButton's glyph, muted at rest and in ink while open", () => {
    renderHelp();
    const trigger = screen.getByRole("button", { name: "help: on Ethernet" });

    expect(trigger).toHaveTextContent("?");
    expect(trigger.parentElement!.className).not.toContain("text-ink");
    fireEvent.click(trigger);
    expect(trigger.parentElement!.className).toContain("[&>button]:text-ink");
  });

  // Middle-aligned, the 24px box centred on the line's x-height and sat the ?
  // below the title it explains; on the baseline, the ? reads as the line's,
  // and pulled in so its box never pushes the line - and its number - down.
  it("sets its ? on the baseline of the line it explains", () => {
    renderHelp();
    const box = screen.getByRole("button", { name: "help: on Ethernet" }).parentElement!;

    expect(box.className).toContain("align-baseline");
    expect(box.className).not.toContain("align-middle");
    expect(box.className).toContain("-my-chrome-y");
  });
});
