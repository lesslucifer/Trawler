import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { General } from "./General";
import type { Invoke } from "./coreService";
import type { KeepCapturing, LoginItem } from "./generalPrefs";

const ENTRY = "/Applications/Trawler.app/Contents/MacOS/trawler-ui";

function shell({
  keepCapturing = "off" as KeepCapturing,
  tray = true,
  login = { on: false, starts: null } as LoginItem,
  refuse = null as string | null,
  updates = { version: "0.1.0-5", installed: true, automatic: true },
} = {}) {
  const stored = { keepCapturing, login, updates };
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
      case "update_prefs_read":
        return stored.updates;
      case "update_automatic_write":
        if (refuse) throw refuse;
        stored.updates = { ...stored.updates, automatic: Boolean(args?.on) };
        return null;
      case "check_for_updates":
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
    const { unmount } = render(<General call={call} os="macos" />);
    expect(await keepBox()).toHaveProperty("checked", ticked);
    expect(screen.getByText("Quit from the menu-bar icon.")).toBeInTheDocument();
    unmount();
  }
});

test("with no menu-bar icon keep capturing is disabled, says why, and drops its line", async () => {
  const { call } = shell({ keepCapturing: "on", tray: false });
  render(<General call={call} os="macos" />);
  const box = await keepBox();
  expect(box).toBeDisabled();
  expect(box).not.toBeChecked();
  expect(screen.getByText("needs a menu-bar icon, which this desktop can't show")).toBeInTheDocument();
  expect(screen.queryByText("Quit from the menu-bar icon.")).not.toBeInTheDocument();
});

test("ticking keep capturing writes it once and shows it on", async () => {
  const { call, spy, stored } = shell();
  render(<General call={call} os="macos" />);
  fireEvent.click(await keepBox());
  await waitFor(async () => expect(await keepBox()).toBeChecked());
  expect(writes(spy, "keep_capturing_write")).toEqual([["keep_capturing_write", { on: true }]]);
  expect(stored.keepCapturing).toBe("on");
});

test("open at login is ticked whenever an entry exists, and names its program on hover", async () => {
  const { call } = shell({ login: { on: true, starts: ENTRY } });
  render(<General call={call} os="macos" />);
  const box = await loginBox();
  expect(box).toBeChecked();
  expect(box.closest("[title]")).toHaveAttribute("title", ENTRY);
  expect(screen.getByText("Opens in the menu bar, with no window.")).toBeInTheDocument();
});

test("off, open at login has nothing to name", async () => {
  const { call } = shell();
  render(<General call={call} os="macos" />);
  const box = await loginBox();
  expect(box).not.toBeChecked();
  expect(box.closest("[title]")).toBeNull();
});

test("ticking open at login shows the entry the shell wrote", async () => {
  const { call, spy } = shell();
  render(<General call={call} os="macos" />);
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
  render(<General call={call} os="macos" />);

  fireEvent.click(await keepBox());
  expect(await screen.findByText("couldn't save — permission denied")).toBeInTheDocument();
  await waitFor(async () => expect(await keepBox()).not.toBeChecked());

  fireEvent.click(await loginBox());
  await waitFor(() => expect(screen.getAllByText("couldn't save — permission denied")).toHaveLength(2));
  await waitFor(async () => expect(await loginBox()).toBeChecked());
});

describe("each OS names its own icon place", () => {
  test.each([
    ["macos", "Quit from the menu-bar icon.", "Opens in the menu bar, with no window."],
    ["windows", "Quit from the tray icon.", "Opens in the system tray (notification area), with no window."],
    ["linux", "Quit from the tray icon.", "Opens in the system tray, with no window."],
  ] as const)("%s, with a tray", async (os, quitLine, loginLine) => {
    const { call } = shell();
    render(<General call={call} os={os} />);
    await keepBox();
    expect(screen.getByText(quitLine)).toBeInTheDocument();
    expect(screen.getByText(loginLine)).toBeInTheDocument();
  });

  test.each([
    ["macos", "needs a menu-bar icon, which this desktop can't show"],
    ["windows", "needs a tray icon, which this desktop can't show"],
    ["linux", "needs a tray icon, which this desktop can't show — on GNOME, the AppIndicator extension shows one"],
  ] as const)("%s, with no tray: the notice, and a login launch opens the window", async (os, notice) => {
    const { call } = shell({ tray: false });
    render(<General call={call} os={os} />);
    await keepBox();
    expect(screen.getByText(notice)).toBeInTheDocument();
    expect(screen.getByText("Opens with its window.")).toBeInTheDocument();
  });
});

const autoBox = () => screen.findByRole("checkbox", { name: "check for updates automatically" });

describe("updates", () => {
  test("General shows the running version, check now and the launch check switch, on by default", async () => {
    const { call, spy } = shell();
    render(<General call={call} os="macos" />);
    expect(await autoBox()).toHaveProperty("checked", true);
    expect(screen.getByRole("button", { name: "copy version" })).toHaveTextContent("0.1.0-5");
    fireEvent.click(screen.getByRole("button", { name: "check now" }));
    expect(writes(spy, "check_for_updates")).toHaveLength(1);
  });

  test("unticking stores the launch check off", async () => {
    const { call, stored } = shell();
    render(<General call={call} os="windows" />);
    fireEvent.click(await autoBox());
    await waitFor(async () => expect(await autoBox()).toHaveProperty("checked", false));
    expect(stored.updates.automatic).toBe(false);
  });

  test("a refused write says why and the box shows what is stored", async () => {
    const { call } = shell({ refuse: "Permission denied (os error 13)" });
    render(<General call={call} os="linux" />);
    fireEvent.click(await autoBox());
    expect(await screen.findByText("couldn't save — permission denied")).toBeInTheDocument();
    expect(await autoBox()).toHaveProperty("checked", true);
  });

  test("a copy that is not installed has no check now and says where updates come from", async () => {
    const { call } = shell({ updates: { version: "0.1.0-5", installed: false, automatic: true } });
    render(<General call={call} os="macos" />);
    const box = await autoBox();
    expect(box).toBeDisabled();
    expect(box).toHaveProperty("checked", false);
    expect(screen.queryByRole("button", { name: "check now" })).not.toBeInTheDocument();
    expect(screen.getByText("updates come with an installed copy of Trawler")).toBeInTheDocument();
  });
});
