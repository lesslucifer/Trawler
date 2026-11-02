import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { Help, NotesShown } from "./Help";

const NOTE = "On Ethernet, change Wi-Fi to Ethernet in the command.";

const renderHelp = (shown?: boolean) => {
  const help = <Help text={NOTE}>In Terminal, run:</Help>;
  return render(shown === undefined ? help : <NotesShown.Provider value={shown}>{help}</NotesShown.Provider>);
};

describe("Help", () => {
  it("keeps the line it explains, and its note off the page while notes are hidden", () => {
    renderHelp(false);

    expect(screen.getByText("In Terminal, run:")).toBeInTheDocument();
    expect(screen.queryByText(NOTE)).toBeNull();
  });

  it("draws its note under the line while notes are shown", () => {
    renderHelp(true);

    expect(screen.getByText("In Terminal, run:")).toBeInTheDocument();
    expect(screen.getByText(NOTE)).toHaveAttribute("data-help");
  });

  it("hides its note outside any screen that shows notes", () => {
    renderHelp();

    expect(screen.queryByText(NOTE)).toBeNull();
  });

  it("carries no control of its own", () => {
    renderHelp(true);

    expect(screen.queryByRole("button")).toBeNull();
  });
});
