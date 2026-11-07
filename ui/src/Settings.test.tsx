import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { RUNNING, coreStub } from "./coreStub";
import type { Invoke } from "./coreService";
import { Settings, SettingsDialog } from "./Settings";

const prefs = vi.fn(async (command: string) => {
  if (command === "keep_capturing_read") return "off";
  if (command === "tray_available") return true;
  if (command === "login_item_read") return { on: false, starts: null };
  if (command === "update_prefs_read") return { version: "0.1.0-5", installed: true, automatic: true };
  throw new Error(`${command} is not stubbed`);
}) as unknown as Invoke;

test("Settings opens on General; ↓ shows the Capture service page and ↑ returns", async () => {
  const core = coreStub({ state: RUNNING });
  render(<Settings call={prefs} bridge={core.bridge} />);

  expect(await screen.findByRole("heading", { name: "General" })).toBeInTheDocument();
  expect(screen.getByRole("tabpanel", { name: "General" })).toBeInTheDocument();
  const general = screen.getByRole("tab", { name: "General" });
  expect(general).toHaveAttribute("aria-selected", "true");

  fireEvent.keyDown(general, { key: "ArrowDown" });
  expect(await screen.findByRole("heading", { name: "Capture service" })).toBeInTheDocument();
  expect(await screen.findByText("running")).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "General" })).not.toBeInTheDocument();

  fireEvent.keyDown(screen.getByRole("tab", { name: "Capture service" }), { key: "ArrowUp" });
  await waitFor(() => expect(screen.getByRole("heading", { name: "General" })).toBeInTheDocument());
});

test("a click on a page name shows that page", async () => {
  const core = coreStub({ state: RUNNING });
  render(<Settings call={prefs} bridge={core.bridge} />);
  await screen.findByRole("heading", { name: "General" });

  fireEvent.click(screen.getByRole("tab", { name: "Capture service" }));
  expect(await screen.findByRole("heading", { name: "Capture service" })).toBeInTheDocument();
});

test("Settings → General shows the version the shell reports", async () => {
  const core = coreStub({ state: RUNNING });
  render(<Settings call={prefs} bridge={core.bridge} />);
  expect(await screen.findByRole("button", { name: "copy version" })).toHaveTextContent("0.1.0-5");
});

test("the Settings dialog is modal, traps Tab inside, closes on Esc or ✕, and gives focus back", async () => {
  const core = coreStub({ state: RUNNING });
  const close = vi.fn();
  const invoker = document.createElement("button");
  document.body.append(invoker);
  invoker.focus();
  const { unmount } = render(<SettingsDialog call={prefs} bridge={core.bridge} onClose={close} />);
  const dialog = screen.getByRole("dialog", { name: "Settings" });
  expect(dialog).toHaveAttribute("aria-modal", "true");
  await screen.findByRole("heading", { name: "General" });
  expect(dialog.contains(document.activeElement)).toBe(true);

  const stops = [...screen.getByTestId("settings-panel").querySelectorAll<HTMLElement>("button, input, [tabindex='0']")];
  stops[stops.length - 1].focus();
  fireEvent.keyDown(stops[stops.length - 1], { key: "Tab" });
  expect(dialog.contains(document.activeElement)).toBe(true);

  fireEvent.keyDown(dialog, { key: "Escape" });
  fireEvent.click(screen.getByRole("button", { name: "close" }));
  expect(close).toHaveBeenCalledTimes(2);
  unmount();
  expect(document.activeElement).toBe(invoker);
  invoker.remove();
});
