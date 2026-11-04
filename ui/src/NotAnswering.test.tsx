import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { RUNNING } from "./coreStub";
import { NotAnswering } from "./NotAnswering";
import { CoreServiceContext, type CoreService } from "./useCoreService";

function withService(service: Partial<CoreService>) {
  const restart = vi.fn(async () => {});
  const onRetry = vi.fn();
  render(
    <CoreServiceContext.Provider value={{ state: null, restarting: false, refusal: null, restart, openSettings: null, ...service }}>
      <NotAnswering retrying={false} onRetry={onRetry} />
    </CoreServiceContext.Provider>,
  );
  return { restart, onRetry };
}

test("an owned core that exited is offered a restart, naming the exit", () => {
  const { restart, onRetry } = withService({ state: { state: "exited", code: 101, signal: null } });
  expect(screen.getByText("It stopped — exit code 101.")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "restart" }));
  expect(restart).toHaveBeenCalledOnce();
  expect(onRetry).not.toHaveBeenCalled();
});

test("a restart in flight holds its button beside what it waits for", () => {
  withService({ state: { state: "starting" }, restarting: true });
  expect(screen.getByRole("button", { name: "restart" })).toBeDisabled();
  expect(screen.getByText("Restarting it…")).toBeInTheDocument();
});

test("a running, attached or unread core keeps try again", () => {
  for (const state of [RUNNING, { state: "attached" as const, api: "http://127.0.0.1:9" }, null]) {
    const { onRetry } = withService({ state });
    fireEvent.click(screen.getByRole("button", { name: "try again" }));
    expect(onRetry).toHaveBeenCalledOnce();
    document.body.innerHTML = "";
  }
});

test("with no app around it, the notice keeps try again", () => {
  const onRetry = vi.fn();
  render(<NotAnswering retrying={false} onRetry={onRetry} />);
  fireEvent.click(screen.getByRole("button", { name: "try again" }));
  expect(onRetry).toHaveBeenCalledOnce();
});
