import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { RUNNING, coreStub } from "./coreStub";
import type { Invoke } from "./coreService";
import { Settings } from "./Settings";

const prefs = vi.fn(async (command: string) => {
  if (command === "keep_capturing_read") return "off";
  if (command === "tray_available") return true;
  if (command === "login_item_read") return { on: false, starts: null };
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
