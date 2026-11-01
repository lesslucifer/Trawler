import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { StepParts } from "./manualSteps";
import type { GuideStep } from "./setup";

const words = () => screen.getByTestId("step-words");

// W17's D1: a flex row that wraps takes the full width, so a ? after it fell to
// a line of its own; a step with help flows as text, the ? after its last word.
describe("a step's words", () => {
  const explained: GuideStep = [
    "In the emulator's Settings, tap",
    { path: ["Security & privacy", "More security & privacy", "Encryption & credentials"] },
    { help: "Chrome trusts it.", topic: "which apps trust it" },
  ];

  it("flow as text where a ? follows them, one space between parts", () => {
    render(<StepParts step={explained} />);

    expect(words().className).not.toMatch(/flex/);
    expect(words().textContent).toBe(
      "In the emulator's Settings, tap Security & privacy → More security & privacy → Encryption & credentials",
    );
    const help = screen.getByTestId("help");
    expect(help.firstElementChild).toBe(words());
    expect(words().nextElementSibling).toContainElement(screen.getByRole("button", { name: "help: which apps trust it" }));
  });

  it("leave out a control the screen cannot run, with no space for it", () => {
    render(<StepParts step={["Open", { open: "trusted-credentials", label: "open Trusted credentials" }, explained[2]]} />);

    expect(words().textContent).toBe("Open");
  });

  it("stay a row of parts where nothing follows them", () => {
    render(<StepParts step={["Save it", { save: "cer" }]} />);

    expect(words().className).toContain("flex-wrap");
  });
});
