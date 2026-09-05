import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { CLOSE_COPY, ProxyCloseDialog } from "./ProxyCloseDialog";
import type { CloseMode } from "./closeConfirm";

// The dialog's bounded contract. It is the app's only modal, and the only screen
// that can strand a machine with no internet, so every mode's action set and
// every route out is asserted here.

const mount = (mode: CloseMode, over: Partial<Parameters<typeof ProxyCloseDialog>[0]> = {}) => {
  const props = {
    mode,
    os: "macos" as const,
    onRemove: vi.fn(),
    onKeep: vi.fn(),
    onAcknowledge: vi.fn(),
    onCancel: vi.fn(),
    ...over,
  };
  render(<ProxyCloseDialog {...props} />);
  return props;
};

const ACTIONABLE: CloseMode[] = ["ours", "foreign"];

describe("the actionable modes — a proxy is set on this machine", () => {
  it.each(ACTIONABLE)("%s offers one action row: a bare quit and the remedy", (mode) => {
    mount(mode);

    expect(screen.getByRole("button", { name: CLOSE_COPY.quit })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: CLOSE_COPY.turnOff })).toBeInTheDocument();
  });

  it.each(ACTIONABLE)("%s never offers 'keep' as a second name for quitting", (mode) => {
    mount(mode);

    expect(screen.queryByRole("button", { name: /keep/i })).toBeNull();
    expect(screen.getByTestId("proxy-close-dialog")).not.toHaveTextContent(/keep it/i);
  });

  it.each(ACTIONABLE)("%s collapses the manual route beneath the action that leads", (mode) => {
    mount(mode);

    expect(screen.getByTestId("proxy-close-manual")).toBeInTheDocument();
    expect(screen.getByText("do it manually instead")).toBeInTheDocument();
  });

  it("quitting leaves the proxy up and records nothing", () => {
    const p = mount("ours");

    fireEvent.click(screen.getByRole("button", { name: CLOSE_COPY.quit }));
    expect(p.onKeep).toHaveBeenCalledOnce();
    expect(p.onRemove).not.toHaveBeenCalled();
  });

  it("turn it off runs the remedy", () => {
    const p = mount("ours");

    fireEvent.click(screen.getByRole("button", { name: CLOSE_COPY.turnOff }));
    expect(p.onRemove).toHaveBeenCalledOnce();
  });

  it("a failed remedy relabels the same slot and opens the manual route — it strands nobody", async () => {
    const onRemove = vi.fn().mockRejectedValue(new Error("elevation declined"));
    mount("ours", { onRemove });

    fireEvent.click(screen.getByRole("button", { name: CLOSE_COPY.turnOff }));

    await waitFor(() => expect(screen.getByTestId("proxy-close-failed")).toBeInTheDocument());
    expect(screen.getByRole("button", { name: CLOSE_COPY.tryAgain })).toBeInTheDocument();
    expect(screen.getByTestId("proxy-close-failed")).toHaveTextContent(CLOSE_COPY.failed);
    // The escape stays one click away rather than the dialog trapping the user.
    expect(screen.getByRole("button", { name: CLOSE_COPY.quit })).toBeInTheDocument();
    expect(screen.getByTestId("proxy-close-manual").querySelector("details")).toHaveAttribute("open");
  });

  it("try again re-runs the remedy", async () => {
    const onRemove = vi.fn().mockRejectedValue(new Error("declined"));
    mount("ours", { onRemove });
    // The slot relabels, so the retry is found by either name.
    const remedy = () =>
      screen.getByRole("button", {
        name: new RegExp(`^(${CLOSE_COPY.turnOff}|${CLOSE_COPY.tryAgain})$`),
      });

    fireEvent.click(remedy());
    await waitFor(() => expect(screen.getByTestId("proxy-close-failed")).toBeInTheDocument());
    fireEvent.click(remedy());

    expect(onRemove).toHaveBeenCalledTimes(2);
  });

  it("never offers the opt-out — this mode has something local to fix", () => {
    mount("ours");
    expect(screen.queryByRole("checkbox")).toBeNull();
  });
});

describe("the remote mode — nothing local to act on", () => {
  it("leads with the steps and offers no remedy", () => {
    mount("remote");

    expect(screen.getByTestId("proxy-close-steps")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: CLOSE_COPY.turnOff })).toBeNull();
    // Nothing to collapse behind: the steps are the whole content.
    expect(screen.queryByTestId("proxy-close-manual")).toBeNull();
  });

  it("its exit is the same bare quit, in the same corner as every other mode", () => {
    mount("remote");
    expect(screen.getByRole("button", { name: CLOSE_COPY.quit })).toBeInTheDocument();
  });

  it("carries the opt-out's value to the acknowledgement", () => {
    const p = mount("remote");

    fireEvent.click(screen.getByRole("checkbox", { name: CLOSE_COPY.remote.dontShowAgain }));
    fireEvent.click(screen.getByRole("button", { name: CLOSE_COPY.quit }));

    expect(p.onAcknowledge).toHaveBeenCalledWith(true);
  });

  it("quits without persisting when the box is left alone", () => {
    const p = mount("remote");

    fireEvent.click(screen.getByRole("button", { name: CLOSE_COPY.quit }));
    expect(p.onAcknowledge).toHaveBeenCalledWith(false);
  });
});

describe("the modal shell — three routes out, one outcome", () => {
  it.each(["ours", "foreign", "remote"] as CloseMode[])("%s: Esc cancels the quit", (mode) => {
    const p = mount(mode);

    fireEvent.keyDown(screen.getByTestId("proxy-close-dialog"), { key: "Escape" });
    expect(p.onCancel).toHaveBeenCalledOnce();
  });

  it.each(["ours", "foreign", "remote"] as CloseMode[])("%s: the backdrop cancels", (mode) => {
    const p = mount(mode);

    fireEvent.click(screen.getByTestId("proxy-close-backdrop"));
    expect(p.onCancel).toHaveBeenCalledOnce();
  });

  it.each(["ours", "foreign", "remote"] as CloseMode[])("%s: the dismiss glyph cancels", (mode) => {
    const p = mount(mode);

    fireEvent.click(screen.getByRole("button", { name: "cancel" }));
    expect(p.onCancel).toHaveBeenCalledOnce();
  });

  it("is a modal with an accessible name", () => {
    mount("ours");

    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveAttribute("aria-label", CLOSE_COPY.ours.title);
  });

  it("takes focus into the panel on open", async () => {
    mount("ours");

    await waitFor(() => {
      expect(screen.getByTestId("proxy-close-dialog").contains(document.activeElement)).toBe(true);
    });
  });

  it("returns focus to the invoker when it unmounts", async () => {
    const invoker = document.createElement("button");
    document.body.appendChild(invoker);
    invoker.focus();

    const { unmount } = render(
      <ProxyCloseDialog
        mode="ours"
        os="macos"
        onRemove={vi.fn()}
        onKeep={vi.fn()}
        onAcknowledge={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    await waitFor(() => expect(document.activeElement).not.toBe(invoker));

    unmount();
    expect(document.activeElement).toBe(invoker);
    invoker.remove();
  });

  it("traps Tab inside the panel — the scrim is not a way out", () => {
    mount("ours");

    const dialog = screen.getByTestId("proxy-close-dialog");
    const stops = [...dialog.querySelectorAll<HTMLElement>("button:not([tabindex='-1'])")];
    const last = stops[stops.length - 1];
    last.focus();

    fireEvent.keyDown(dialog, { key: "Tab" });
    expect(dialog.contains(document.activeElement)).toBe(true);
  });
});
