import { describe, expect, it, vi } from "vitest";
import { createEvent, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { CLOSE_COPY, deviceShown, ProxyCloseDialog } from "./ProxyCloseDialog";
import type { QuitPrompt } from "./closeConfirm";
import { proxyOffSnippet } from "./proxyTargets";
import { routeUndo, type HostOs } from "./setup";

// The dialog's bounded contract. It is the app's only modal, and the only screen
// that can strand a machine with no internet, so every prompt's answers and
// every route out is asserted here.

const OURS: QuitPrompt = { mode: "ours", answering: true };
const STALE: QuitPrompt = { mode: "ours", answering: false };
const UNREAD: QuitPrompt = { mode: "unread" };
const DEVICES: QuitPrompt = { mode: "devices", emulator: null };
const EMULATOR: QuitPrompt = { mode: "devices", emulator: "Pixel_7_API_35" };
const EVERY: [string, QuitPrompt][] = [
  ["ours", OURS],
  ["unread", UNREAD],
  ["devices", DEVICES],
];

const mount = (prompt: QuitPrompt, over: Partial<Parameters<typeof ProxyCloseDialog>[0]> = {}) => {
  const props = {
    prompt,
    os: "macos" as HostOs,
    chosen: "this" as const,
    onStop: vi.fn(),
    onQuit: vi.fn(),
    onCancel: vi.fn(),
    ...over,
  };
  render(<ProxyCloseDialog {...props} />);
  return props;
};

const stopButton = () => screen.getByRole("button", { name: CLOSE_COPY.stop });

// The same root and selector the trap uses, so these tests exercise the real
// stop list rather than one that happens to share its edges.
const FOCUSABLE =
  'a[href], button:not([disabled]), summary, input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';
const tabStops = () => [
  ...screen.getByTestId("proxy-close-panel").querySelectorAll<HTMLElement>(FOCUSABLE),
];

describe("this machine routes through Trawler", () => {
  it("asks in setup's words, with a bare quit and the button that stops routing (S3)", () => {
    mount(OURS);

    expect(screen.getByRole("dialog")).toHaveAttribute("aria-label", "stop routing through Trawler before quitting?");
    expect(screen.getByRole("button", { name: CLOSE_COPY.quit })).toBeInTheDocument();
    expect(stopButton()).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /turn/i })).toBeNull();
    expect(screen.queryByText(/the proxy is off/i)).toBeNull();
  });

  it("names the capture service as the stake, not the window (D5)", () => {
    mount(OURS);
    expect(screen.getByTestId("proxy-close-panel")).toHaveTextContent(CLOSE_COPY.stake.ours("this Mac"));
  });

  it("says this machine has no internet now while the capture service does not answer (D5)", () => {
    mount(STALE);
    expect(screen.getByTestId("proxy-close-panel")).toHaveTextContent(CLOSE_COPY.stake.stale("this Mac"));
  });

  it("collapses this machine's own steps beneath the button that leads (S4)", () => {
    mount(OURS);

    expect(screen.getByText(CLOSE_COPY.byHand)).toBeInTheDocument();
    expect(within(screen.getByTestId("proxy-close-manual")).getByText(proxyOffSnippet("macos"))).toBeInTheDocument();
  });

  it("on Linux, where the shell cannot stop it, shows the command inline and no button (D2)", () => {
    mount(OURS, { os: "linux" });

    expect(screen.queryByRole("button", { name: CLOSE_COPY.stop })).toBeNull();
    expect(screen.queryByTestId("proxy-close-manual")).toBeNull();
    expect(within(screen.getByTestId("proxy-close-steps")).getByText(proxyOffSnippet("linux"))).toBeInTheDocument();
    expect(screen.getByTestId("proxy-close-steps")).toHaveTextContent("In a terminal, run:");
    expect(screen.getByTestId("proxy-close-panel").textContent).not.toMatch(/GNOME/);
    expect(screen.getByTestId("proxy-close-panel")).toHaveTextContent(CLOSE_COPY.stake.ours("this machine"));
  });

  it("quitting leaves the proxy up and persists nothing", () => {
    const p = mount(OURS);

    fireEvent.click(screen.getByRole("button", { name: CLOSE_COPY.quit }));
    expect(p.onQuit).toHaveBeenCalledWith(false);
    expect(p.onStop).not.toHaveBeenCalled();
  });

  it("the button stops routing, naming a prompt only on macOS (S3)", () => {
    const p = mount(OURS, { onStop: vi.fn(() => new Promise<void>(() => {})) });

    fireEvent.click(stopButton());
    expect(p.onStop).toHaveBeenCalledOnce();
    expect(screen.getByTestId("proxy-close-running")).toHaveTextContent(CLOSE_COPY.stopping("macos"));
    expect(CLOSE_COPY.stopping("windows")).not.toMatch(/prompt/);
  });

  it("a failure relabels the same slot and opens the steps — it strands nobody", async () => {
    const onStop = vi.fn().mockRejectedValue(new Error("elevation declined"));
    mount(OURS, { onStop });

    fireEvent.click(stopButton());

    await waitFor(() => expect(screen.getByTestId("proxy-close-failed")).toBeInTheDocument());
    expect(screen.getByRole("button", { name: CLOSE_COPY.tryAgain })).toBeInTheDocument();
    expect(screen.getByTestId("proxy-close-failed")).toHaveTextContent(CLOSE_COPY.failed);
    expect(screen.getByRole("button", { name: CLOSE_COPY.quit })).toBeInTheDocument();
    expect(screen.getByTestId("proxy-close-manual").querySelector("details")).toHaveAttribute("open");
  });

  it("try again re-runs the stop", async () => {
    const onStop = vi.fn().mockRejectedValue(new Error("declined"));
    mount(OURS, { onStop });

    fireEvent.click(stopButton());
    await waitFor(() => expect(screen.getByTestId("proxy-close-failed")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: CLOSE_COPY.tryAgain }));

    expect(onStop).toHaveBeenCalledTimes(2);
  });

  // stop() resolves after the unset succeeds AND the window destroy settles —
  // and the destroy failing is deliberately swallowed, so a resolved stop can
  // leave the window open. Routing has stopped, so the dialog says so, and what
  // remains is the way out: the button is gone, not disabled (S1).
  it("once it worked and the window stayed, the button gives way to its outcome (S1)", async () => {
    const onStop = vi.fn().mockResolvedValue(undefined);
    mount(OURS, { onStop });

    const button = stopButton();
    button.focus();
    fireEvent.click(button);

    await waitFor(() => expect(screen.getByTestId("proxy-close-done")).toBeInTheDocument());
    expect(screen.getByTestId("proxy-close-done")).toHaveTextContent(CLOSE_COPY.stopped("this Mac"));
    expect(screen.queryByTestId("proxy-close-running")).toBeNull();
    expect(screen.queryByRole("button", { name: CLOSE_COPY.stop })).toBeNull();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: CLOSE_COPY.quit }));
    expect(onStop).toHaveBeenCalledOnce();
  });

  it("never offers the opt-out — there is something here to fix", () => {
    mount(OURS);
    expect(screen.queryByRole("checkbox")).toBeNull();
  });
});

describe("this machine's proxy could not be read (D2)", () => {
  it("says so, never that no proxy is set, and offers its steps inline with no button", () => {
    mount(UNREAD);

    expect(screen.getByRole("dialog")).toHaveAttribute("aria-label", CLOSE_COPY.title.unread("this Mac"));
    expect(screen.getByTestId("proxy-close-panel")).toHaveTextContent(CLOSE_COPY.stake.unread("this Mac"));
    expect(screen.getByTestId("proxy-close-panel")).not.toHaveTextContent(/no proxy is set/i);
    expect(screen.queryByRole("button", { name: CLOSE_COPY.stop })).toBeNull();
    expect(within(screen.getByTestId("proxy-close-steps")).getByText(proxyOffSnippet("macos"))).toBeInTheDocument();
  });

  it("carries the opt-out's value to the quit", () => {
    const p = mount(UNREAD);

    fireEvent.click(screen.getByRole("checkbox", { name: CLOSE_COPY.dontShowAgain }));
    fireEvent.click(screen.getByRole("button", { name: CLOSE_COPY.quit }));
    expect(p.onQuit).toHaveBeenCalledWith(true);
  });
});

describe("nothing here routes through Trawler, but a device could (D4)", () => {
  it("leads with a device's own steps, in the same frame, with no button", () => {
    mount(DEVICES);

    expect(screen.getByTestId("proxy-close-panel")).toHaveTextContent(CLOSE_COPY.stake.devices);
    expect(screen.queryByRole("button", { name: CLOSE_COPY.stop })).toBeNull();
    expect(screen.queryByTestId("proxy-close-manual")).toBeNull();
    expect(screen.getByTestId("proxy-close-steps")).toHaveTextContent(
      routeUndo("ios", "macos")[0][0] as string,
    );
  });

  it("names the emulator it found and opens on its steps", () => {
    mount(EMULATOR);

    expect(screen.getByTestId("proxy-close-panel")).toHaveTextContent(CLOSE_COPY.stake.emulator("Pixel_7_API_35"));
    expect(screen.getByRole("button", { name: "steps for" })).toHaveTextContent("Android emulator");
  });

  it("offers the platforms a device could be, and the steps setup gives each (S4)", () => {
    mount(DEVICES);

    fireEvent.click(screen.getByRole("button", { name: "steps for" }));
    const offered = screen.getAllByRole("option").map((o) => o.textContent?.replace("✓", ""));
    expect(offered).toEqual([
      "Another Mac",
      "Another Windows PC",
      "Another Ubuntu machine",
      "Android emulator",
      "iPhone or iPad",
      "Android device",
    ]);
    fireEvent.click(screen.getByRole("option", { name: "Android device" }));
    expect(screen.getByTestId("proxy-close-steps")).toHaveTextContent(
      routeUndo("android", "macos")[0][0] as string,
    );
  });

  it("carries the opt-out's value to the quit, and quits without it when left alone", () => {
    const ticked = mount(DEVICES);
    fireEvent.click(screen.getByRole("checkbox", { name: CLOSE_COPY.dontShowAgain }));
    fireEvent.click(screen.getByRole("button", { name: CLOSE_COPY.quit }));
    expect(ticked.onQuit).toHaveBeenCalledWith(true);
  });
});

describe("which device's steps open first", () => {
  it("the emulator found, else the device last set up, else an iPhone", () => {
    expect(deviceShown(EMULATOR, "ios")).toBe("android-sim");
    expect(deviceShown(DEVICES, "android")).toBe("android");
    expect(deviceShown(DEVICES, "this")).toBe("ios");
    expect(deviceShown(DEVICES, "ios-sim")).toBe("ios");
  });
});

describe("the modal shell — three routes out, one outcome", () => {
  it.each(EVERY)("%s: Esc cancels the quit", (_, prompt) => {
    const p = mount(prompt);

    fireEvent.keyDown(screen.getByTestId("proxy-close-dialog"), { key: "Escape" });
    expect(p.onCancel).toHaveBeenCalledOnce();
  });

  it.each(EVERY)("%s: the backdrop cancels", (_, prompt) => {
    const p = mount(prompt);

    fireEvent.click(screen.getByTestId("proxy-close-backdrop"));
    expect(p.onCancel).toHaveBeenCalledOnce();
  });

  it.each(EVERY)("%s: the dismiss glyph cancels", (_, prompt) => {
    const p = mount(prompt);

    fireEvent.click(screen.getByRole("button", { name: "cancel" }));
    expect(p.onCancel).toHaveBeenCalledOnce();
  });

  it.each(EVERY)("%s: one frame — a single dialog, a single stake (S2)", (_, prompt) => {
    mount(prompt);

    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    expect(screen.getByTestId("proxy-close-dialog")).toHaveAttribute("data-mode", prompt.mode);
  });

  it("counts the disclosure summary as a stop — the trap must match the tab ring", () => {
    mount(OURS);

    const summary = screen.getByTestId("proxy-close-panel").querySelector("summary");
    expect(summary).not.toBeNull();

    summary!.focus();
    const forward = createEvent.keyDown(screen.getByTestId("proxy-close-dialog"), { key: "Tab" });
    fireEvent(screen.getByTestId("proxy-close-dialog"), forward);

    expect(forward.defaultPrevented).toBe(false);
  });

  // Select closes its popup on Escape with preventDefault but no stopPropagation,
  // so the event still reaches the shell. Closing a dropdown must not cancel the
  // quit — the user asked to dismiss the list, not the dialog.
  it("Escape that closed a dropdown does not also cancel the quit", () => {
    const p = mount(DEVICES);

    fireEvent.click(screen.getByRole("button", { name: "steps for" }));
    expect(screen.getByRole("listbox")).toBeInTheDocument();

    const escape = createEvent.keyDown(screen.getByRole("listbox"), { key: "Escape" });
    Object.defineProperty(escape, "defaultPrevented", { value: true });
    fireEvent(screen.getByTestId("proxy-close-dialog"), escape);

    expect(p.onCancel).not.toHaveBeenCalled();
  });

  // The ✕ is the first stop in the DOM and the manual-steps summary is the next;
  // opening onto either would put the escape, or an answer to nothing, under the
  // first Enter.
  it.each([
    ["ours", OURS, CLOSE_COPY.stop],
    ["ours on Linux", OURS, CLOSE_COPY.quit, "linux"],
    ["unread", UNREAD, CLOSE_COPY.quit],
    ["devices", DEVICES, CLOSE_COPY.quit],
  ] as [string, QuitPrompt, string, HostOs?][])("%s: opens with focus on the answer it asks for", async (_, prompt, action, os) => {
    mount(prompt, os ? { os } : {});

    await waitFor(() => {
      expect(screen.getByTestId("proxy-close-panel").contains(document.activeElement)).toBe(true);
    });
    expect(document.activeElement).toBe(screen.getByRole("button", { name: action }));
  });

  it("returns focus to the invoker when it unmounts", async () => {
    const invoker = document.createElement("button");
    document.body.appendChild(invoker);
    invoker.focus();

    const { unmount } = render(
      <ProxyCloseDialog prompt={OURS} os="macos" chosen="this" onStop={vi.fn()} onQuit={vi.fn()} onCancel={vi.fn()} />,
    );
    await waitFor(() => expect(document.activeElement).not.toBe(invoker));

    unmount();
    expect(document.activeElement).toBe(invoker);
    invoker.remove();
  });

  // jsdom does not move focus on Tab, so asserting "focus is still inside" would
  // hold with the trap deleted. The discriminating facts are that the wrap was
  // taken (the event is defaultPrevented) and WHERE focus landed.
  it("wraps focus from the last stop back to the first — the scrim is not a way out", () => {
    mount(OURS);

    const dialog = screen.getByTestId("proxy-close-dialog");
    const stops = tabStops();
    stops[stops.length - 1].focus();

    const forward = createEvent.keyDown(dialog, { key: "Tab" });
    fireEvent(dialog, forward);

    expect(forward.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(stops[0]);
  });

  it("wraps backwards from the first stop to the last on shift+Tab", () => {
    mount(OURS);

    const dialog = screen.getByTestId("proxy-close-dialog");
    const stops = tabStops();
    stops[0].focus();

    const backward = createEvent.keyDown(dialog, { key: "Tab", shiftKey: true });
    fireEvent(dialog, backward);

    expect(backward.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(stops[stops.length - 1]);
  });

  it("leaves Tab alone in the middle of the panel — only the edges wrap", () => {
    mount(OURS);

    const dialog = screen.getByTestId("proxy-close-dialog");
    const stops = tabStops();
    stops[0].focus();

    const forward = createEvent.keyDown(dialog, { key: "Tab" });
    fireEvent(dialog, forward);

    expect(forward.defaultPrevented).toBe(false);
  });
});
