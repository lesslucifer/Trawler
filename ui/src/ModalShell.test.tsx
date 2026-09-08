import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { ModalShell } from "./ModalShell";

// The frame two screens now share. ProxyCloseDialog's own suite proves it in the
// composition it shipped in; these prove the contract the second and third
// sightings depend on — including the one behaviour the lift added, a marker on
// something that is not a button.
function mount(children: React.ReactNode, onCancel = vi.fn()) {
  render(
    <ModalShell title="clear 6 flows?" onCancel={onCancel} testId="shell">
      {children}
    </ModalShell>,
  );
  return onCancel;
}

test("initial focus goes to whatever the caller marked, not to the first thing in the DOM", () => {
  mount(
    <>
      <button type="button">act</button>
      <span data-opener="true">
        <button type="button">cancel</button>
      </span>
    </>,
  );
  expect(document.activeElement).toHaveTextContent("cancel");
});

test("the marker may be the focusable itself — the paste target is a textarea, not a button", () => {
  mount(
    <>
      <textarea data-opener="true" aria-label="a curl command" />
      <button type="button">import</button>
    </>,
  );
  expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "a curl command" }));
});

test("with nothing marked, focus falls to the panel's first focusable — the ✕", () => {
  mount(<p>nothing focusable of my own</p>);
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "cancel" }));
});

test("the ✕'s name is the caller's, so two controls never dismiss under one name", () => {
  render(
    <ModalShell title="t" onCancel={vi.fn()} testId="shell" dismissLabel="close">
      <button type="button">cancel</button>
    </ModalShell>,
  );
  expect(screen.getByRole("button", { name: "close" })).toBeInTheDocument();
});

test.each([
  ["Esc", () => fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" })],
  ["the backdrop", () => fireEvent.click(screen.getByTestId("shell-backdrop"))],
  ["the ✕", () => fireEvent.click(screen.getByRole("button", { name: "cancel" }))],
])("%s dismisses the dialog", (_name, dismiss) => {
  const onCancel = mount(<p>body</p>);
  dismiss();
  expect(onCancel).toHaveBeenCalledOnce();
});

test("an Esc a nested control already consumed is not the dialog closing", () => {
  // A Select inside the panel closing its own popup marks the event handled.
  // Dismissing a dropdown is not dismissing the dialog around it.
  const onCancel = vi.fn();
  render(
    <ModalShell title="t" onCancel={onCancel} testId="shell">
      <button type="button" onKeyDown={(e) => e.preventDefault()}>
        a control with a popup of its own
      </button>
    </ModalShell>,
  );
  fireEvent.keyDown(screen.getByRole("button", { name: /popup of its own/ }), { key: "Escape" });
  expect(onCancel).not.toHaveBeenCalled();

  fireEvent.keyDown(screen.getByTestId("shell-panel"), { key: "Escape" });
  expect(onCancel).toHaveBeenCalledOnce();
});

test("Tab wraps inside the panel rather than escaping the modal", () => {
  mount(
    <span data-opener="true">
      <button type="button">only</button>
    </span>,
  );
  const stops = [...screen.getByTestId("shell-panel").querySelectorAll("button")];
  stops[stops.length - 1].focus();
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Tab" });
  expect(document.activeElement).toBe(stops[0]);
});

test("closing returns focus to whatever opened it", () => {
  const invoker = document.createElement("button");
  document.body.append(invoker);
  invoker.focus();

  const { unmount } = render(
    <ModalShell title="t" onCancel={vi.fn()} testId="shell">
      <button type="button">act</button>
    </ModalShell>,
  );
  expect(document.activeElement).not.toBe(invoker);
  unmount();
  expect(document.activeElement).toBe(invoker);
  invoker.remove();
});

test("each dialog's parts are addressable under its own name", () => {
  mount(<p>body</p>);
  expect(screen.getByTestId("shell-dialog")).toHaveAttribute("aria-modal", "true");
  expect(screen.getByTestId("shell-panel")).toBeInTheDocument();
  expect(screen.getByTestId("shell-backdrop")).toBeInTheDocument();
});
