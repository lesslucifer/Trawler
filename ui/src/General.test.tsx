import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { General } from "./General";
import type { Invoke } from "./coreService";
import type { KeepCapturing, LoginItem } from "./generalPrefs";

const ENTRY = "/Applications/Trawler.app/Contents/MacOS/trawler-ui";

function shell({
  keepCapturing = "off" as KeepCapturing,
  tray = true,
  login = { on: false, starts: null } as LoginItem,
  refuse = null as string | null,
} = {}) {
  const stored = { keepCapturing, login };
  const call = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    switch (command) {
      case "keep_capturing_read":
        return stored.keepCapturing;
      case "tray_available":
        return tray;
      case "login_item_read":
        return stored.login;
      case "keep_capturing_write":
        if (refuse) throw refuse;
        stored.keepCapturing = args?.on ? "on" : "off";
        return null;
      case "login_item_write":
        if (refuse) throw refuse;
        stored.login = args?.on ? { on: true, starts: ENTRY } : { on: false, starts: null };
        return stored.login;
      default:
        throw new Error(`${command} is not stubbed`);
    }
  });
  return { call: call as unknown as Invoke, spy: call, stored };
}

const keepBox = () => screen.findByRole("checkbox", { name: "keep capturing when the window closes" });
const loginBox = () => screen.findByRole("checkbox", { name: "open Trawler at login" });
const writes = (spy: ReturnType<typeof vi.fn>, command: string) =>
  spy.mock.calls.filter(([called]) => called === command);

test("keep capturing is ticked only when the shell reads it on", async () => {
  for (const [reading, ticked] of [
    ["on", true],
    ["off", false],
    ["unanswered", false],
  ] as const) {
    const { call } = shell({ keepCapturing: reading });
    const { unmount } = render(<General call={call} />);
    expect(await keepBox()).toHaveProperty("checked", ticked);
    expect(screen.getByText("Quit from the menu-bar icon.")).toBeInTheDocument();
    unmount();
  }
});

test("with no menu-bar icon keep capturing is disabled, says why, and drops its line", async () => {
  const { call } = shell({ keepCapturing: "on", tray: false });
  render(<General call={call} />);
  const box = await keepBox();
  expect(box).toBeDisabled();
  expect(box).not.toBeChecked();
  expect(screen.getByText("needs a menu-bar icon, which this desktop can't show")).toBeInTheDocument();
  expect(screen.queryByText("Quit from the menu-bar icon.")).not.toBeInTheDocument();
});

test("ticking keep capturing writes it once and shows it on", async () => {
  const { call, spy, stored } = shell();
  render(<General call={call} />);
  fireEvent.click(await keepBox());
  await waitFor(async () => expect(await keepBox()).toBeChecked());
  expect(writes(spy, "keep_capturing_write")).toEqual([["keep_capturing_write", { on: true }]]);
  expect(stored.keepCapturing).toBe("on");
});

test("open at login is ticked whenever an entry exists, and names its program on hover", async () => {
  const { call } = shell({ login: { on: true, starts: ENTRY } });
  render(<General call={call} />);
  const box = await loginBox();
  expect(box).toBeChecked();
  expect(box.closest("[title]")).toHaveAttribute("title", ENTRY);
  expect(screen.getByText("Opens in the menu bar, with no window.")).toBeInTheDocument();
});

test("off, open at login has nothing to name", async () => {
  const { call } = shell();
  render(<General call={call} />);
  const box = await loginBox();
  expect(box).not.toBeChecked();
  expect(box.closest("[title]")).toBeNull();
});

test("ticking open at login shows the entry the shell wrote", async () => {
  const { call, spy } = shell();
  render(<General call={call} />);
  fireEvent.click(await loginBox());
  await waitFor(async () => expect(await loginBox()).toBeChecked());
  expect((await loginBox()).closest("[title]")).toHaveAttribute("title", ENTRY);
  expect(writes(spy, "login_item_write")).toEqual([["login_item_write", { on: true }]]);
});

test("a refused write says why and each box shows what is stored", async () => {
  const { call } = shell({
    keepCapturing: "off",
    login: { on: true, starts: ENTRY },
    refuse: "Permission denied (os error 13)",
  });
  render(<General call={call} />);

  fireEvent.click(await keepBox());
  expect(await screen.findByText("couldn't save — permission denied")).toBeInTheDocument();
  await waitFor(async () => expect(await keepBox()).not.toBeChecked());

  fireEvent.click(await loginBox());
  await waitFor(() => expect(screen.getAllByText("couldn't save — permission denied")).toHaveLength(2));
  await waitFor(async () => expect(await loginBox()).toBeChecked());
});
