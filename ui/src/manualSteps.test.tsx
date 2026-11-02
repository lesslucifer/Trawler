import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { NotesShown } from "./components/index";
import { StepParts } from "./manualSteps";
import type { GuideStep } from "./setup";

const words = () => screen.getByTestId("step-words");

// DEC-66: a step's note is drawn under its words while notes are shown, so the
// words keep the one row every step has.
describe("a step's words", () => {
  const explained: GuideStep = [
    "In the emulator's Settings, tap",
    { path: ["Security & privacy", "More security & privacy", "Encryption & credentials"] },
    { help: "Chrome trusts it." },
  ];

  it("stay a row of parts with the note under them while notes are shown", () => {
    render(
      <NotesShown.Provider value={true}>
        <StepParts step={explained} />
      </NotesShown.Provider>,
    );

    expect(words().className).toContain("flex-wrap");
    expect(words()).toHaveTextContent("In the emulator's Settings, tap");
    expect(screen.getByTestId("help").firstElementChild).toBe(words());
    expect(words().nextElementSibling).toHaveTextContent("Chrome trusts it.");
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("carry no note while notes are hidden", () => {
    render(<StepParts step={explained} />);

    expect(screen.queryByText("Chrome trusts it.")).toBeNull();
  });

  it("leave out a control the screen cannot run", () => {
    render(<StepParts step={["Open", { open: "trusted-credentials", label: "open Trusted credentials" }, explained[2]]} />);

    expect(words().textContent).toBe("Open");
  });
});

// W17's S1: a route to a device that is not running stands in place, waiting,
// and frames nothing.
describe("a route that waits for its device", () => {
  it("is disabled and never the framed control", () => {
    render(
      <StepParts
        step={["Send it", { open: "emulator-certificate", label: "send it to the emulator", disabled: true }]}
        open={() => undefined}
        framed
      />,
    );

    const send = screen.getByRole("button", { name: "send it to the emulator" });
    expect(send).toBeDisabled();
    expect(send.className).not.toContain("border-hairline");
  });
});

// W19: a code is what the step's sentence asks the camera to scan, so it sits
// under that sentence, and the recording of what follows comes after it.
describe("a step with a code and a recording", () => {
  it("draws the code before the recording", () => {
    render(
      <StepParts
        step={[
          "Scan with the camera, or open",
          { copy: "http://192.168.1.24:7421/android-wifi", label: "the Wi-Fi page", wire: "wifi page url", scan: true },
          { media: "android-wifi-proxy", alt: "Settings: Proxy", shows: "Android 15" },
        ]}
      />,
    );

    const code = screen.getByTestId("scan-code");
    const media = screen.getByTestId("step-media");
    expect(code.compareDocumentPosition(media) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
