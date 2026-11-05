import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { KeepCapturingDialog } from "./KeepCapturingDialog";

function shown() {
  const answers = { onKeep: vi.fn(), onQuit: vi.fn(), onCancel: vi.fn() };
  render(<KeepCapturingDialog {...answers} />);
  return answers;
}

describe("the once-question", () => {
  test("says what W5's design says", () => {
    shown();
    expect(screen.getByRole("heading", { name: "keep capturing after the window closes?" })).toBeInTheDocument();
    expect(
      screen.getByText("Trawler stays in the menu bar and keeps capturing. You can change this in Settings → General."),
    ).toBeInTheDocument();
  });

  test("focus lands on keep capturing, not on the escape", () => {
    shown();
    expect(screen.getByRole("button", { name: "keep capturing" })).toHaveFocus();
  });

  test("each answer reports once and nothing else", () => {
    const answers = shown();
    fireEvent.click(screen.getByRole("button", { name: "keep capturing" }));
    expect(answers.onKeep).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: "quit" }));
    expect(answers.onQuit).toHaveBeenCalledOnce();
    expect(answers.onCancel).not.toHaveBeenCalled();
  });

  test("✕ and Esc cancel", () => {
    const answers = shown();
    fireEvent.click(screen.getByRole("button", { name: "cancel" }));
    fireEvent.keyDown(screen.getByTestId("keep-capturing-dialog"), { key: "Escape" });
    expect(answers.onCancel).toHaveBeenCalledTimes(2);
    expect(answers.onKeep).not.toHaveBeenCalled();
    expect(answers.onQuit).not.toHaveBeenCalled();
  });
});
