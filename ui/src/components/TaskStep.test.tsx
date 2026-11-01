import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { TaskStep } from "./TaskStep";

describe("TaskStep", () => {
  it("renders its number and its title — the number is the reading order", () => {
    render(<TaskStep n={2} title="Send this Mac's traffic to Trawler" />);

    expect(screen.getByTestId("task-2")).toHaveTextContent("2");
    expect(screen.getByText("Send this Mac's traffic to Trawler")).toBeInTheDocument();
  });

  it("keeps why the task matters behind a ? on its title, shown only when asked", () => {
    const { rerender } = render(
      <TaskStep n={2} title="Send this Mac's traffic to Trawler" help="its system proxy points at Trawler while Trawler runs" />,
    );
    expect(screen.queryByText("its system proxy points at Trawler while Trawler runs")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "help: why this step" }));
    expect(screen.getByText("its system proxy points at Trawler while Trawler runs")).toBeInTheDocument();

    rerender(<TaskStep n={2} title="Send this Mac's traffic to Trawler" />);
    expect(screen.queryByRole("button", { name: "help: why this step" })).toBeNull();
  });

  it("renders the state it is handed as a StatusLine, glyph and all", () => {
    render(
      <TaskStep
        n={1}
        title="Trust Trawler's certificate"
        status={{ glyph: "✓", tone: "ok", short: "done — installed and trusted" }}
      />,
    );

    expect(screen.getByText("done — installed and trusted")).toBeInTheDocument();
    expect(screen.getByText("✓")).toBeInTheDocument();
  });

  it("renders no state at all where nothing observed one — never a ✓ of its own", () => {
    const { container } = render(<TaskStep n={3} title="Open any website on this Mac" />);

    expect(container.textContent).toBe("3Open any website on this Mac");
  });

  // The e2e reads a task's state through this hook, so its presence is the
  // contract and its absence where nothing was observed is the same contract.
  it("names the state's own element after the task it belongs to, and omits it with the state", () => {
    const { rerender } = render(
      <TaskStep
        n={2}
        title="Send this Mac's traffic to Trawler"
        status={{ glyph: "○", tone: "muted", short: "not done — the system proxy is off" }}
      />,
    );

    expect(screen.getByTestId("task-2-state")).toHaveTextContent("not done — the system proxy is off");

    rerender(<TaskStep n={2} title="Send this Mac's traffic to Trawler" />);
    expect(screen.queryByTestId("task-2-state")).toBeNull();
  });

  it("dims a task whose precondition is not met, and keeps its number", () => {
    render(<TaskStep n={2} title="On your iPhone, install the certificate" waitsOn="after task 1" dim />);

    const task = screen.getByTestId("task-2");
    expect(task.className).toContain("opacity-disabled");
    expect(task).toHaveTextContent("2");
    expect(task).toHaveTextContent("after task 1");
  });

  it("an undimmed task carries no dimming at all", () => {
    render(<TaskStep n={1} title="Allow your iPhone to connect" />);

    expect(screen.getByTestId("task-1").className).not.toContain("opacity-disabled");
  });

  it("the slot holds what the user does, below the state", () => {
    render(
      <TaskStep
        n={2}
        title="Send this Mac's traffic to Trawler"
        status={{ glyph: "○", tone: "muted", short: "not done — the system proxy is off" }}
      >
        <button type="button">Set the system proxy</button>
      </TaskStep>,
    );

    const task = screen.getByTestId("task-2");
    const state = screen.getByText("not done — the system proxy is off");
    const action = screen.getByRole("button", { name: "Set the system proxy" });

    expect(task).toContainElement(action);
    expect(state.compareDocumentPosition(action) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
