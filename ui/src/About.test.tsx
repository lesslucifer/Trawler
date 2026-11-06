import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { About, shownAtOpen, type AboutShow } from "./About";
import type { Invoke } from "./coreService";
import { ABOUT_SHOW_GLOBAL } from "./page";
import { PRIVACY_CLOSE, PRIVACY_LEAD, PRIVACY_ROWS } from "./privacy";

const NOTICE = "MIT License\n\nCopyright (c) 2026 Vu Luu\n\n── axum 0.8.4 ── MIT";
const GUIDE = "https://github.com/lesslucifer/Trawler/blob/main/GUIDE.md";

function shell({ notice = NOTICE as string | null, refuse = null as string | null } = {}) {
  const call = vi.fn(async (command: string) => {
    if (command === "about_read") return { version: "0.1.0-5", notice };
    if (command === "open_link") {
      if (refuse) throw refuse;
      return null;
    }
    throw new Error(`${command} is not stubbed`);
  });
  return { call: call as unknown as Invoke, spy: call };
}

const quiet = () => () => {};
const show = (page: AboutShow["page"], unopened: string | null = null): AboutShow => ({ page, unopened });

test("the About page names Trawler, its version, licence and copyright, and offers three links", async () => {
  const { call } = shell();
  render(<About call={call} initial={show("about")} subscribe={quiet} close={() => {}} />);
  expect(await screen.findByRole("heading", { name: "Trawler" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "copy version" })).toHaveTextContent("0.1.0-5");
  expect(screen.getByText("MIT licence · © 2026 Vu Luu")).toBeInTheDocument();
  for (const label of ["user guide", "report an issue", "source code"]) {
    expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
  }
  expect(screen.getByRole("tab", { name: "About" })).toHaveAttribute("aria-selected", "true");
});

test("each link button names its link to the shell, never an address", async () => {
  const { call, spy } = shell();
  render(<About call={call} initial={show("about")} subscribe={quiet} close={() => {}} />);
  for (const [label, link] of [
    ["user guide", "guide"],
    ["report an issue", "issue"],
    ["source code", "source"],
  ]) {
    fireEvent.click(await screen.findByRole("button", { name: label }));
    expect(spy).toHaveBeenLastCalledWith("open_link", { link });
  }
});

test("a browser that would not open shows the address to copy", async () => {
  const { call } = shell({ refuse: GUIDE });
  render(<About call={call} initial={show("about")} subscribe={quiet} close={() => {}} />);
  fireEvent.click(await screen.findByRole("button", { name: "user guide" }));
  expect(await screen.findByText("couldn't open your browser — copy the address")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "copy address" })).toHaveTextContent(GUIDE);
});

test("the licences page shows the bundled notice verbatim in its own scrolling region", async () => {
  const { call } = shell();
  render(<About call={call} initial={show("licences")} subscribe={quiet} close={() => {}} />);
  const region = await screen.findByRole("region", { name: "Third-party licences" });
  expect(region.textContent).toBe(NOTICE);
  expect(region).toHaveAttribute("tabindex", "0");
});

test("without a bundled notice the licences page says where the list comes from", async () => {
  const { call } = shell({ notice: null });
  render(<About call={call} initial={show("licences")} subscribe={quiet} close={() => {}} />);
  expect(await screen.findByText("the list comes with an installed copy of Trawler")).toBeInTheDocument();
  expect(screen.queryByRole("region", { name: "Third-party licences" })).not.toBeInTheDocument();
});

test("the privacy page shows the one privacy statement, the launch check and its switch included", async () => {
  const { call } = shell();
  render(<About call={call} initial={show("privacy")} subscribe={quiet} close={() => {}} />);
  expect(await screen.findByText(PRIVACY_LEAD)).toBeInTheDocument();
  for (const { what, where } of PRIVACY_ROWS) {
    expect(screen.getByText(what)).toBeInTheDocument();
    expect(screen.getByText(where)).toBeInTheDocument();
  }
  expect(screen.getByText(PRIVACY_CLOSE)).toBeInTheDocument();
  const update = PRIVACY_ROWS.find((row) => row.what === "Update check")!.where;
  expect(update).toContain("when Trawler opens (at most once a day)");
  expect(update).toContain("check for updates automatically");
});

test("the shell can turn the open window to another page, with an address it could not open", async () => {
  const { call } = shell();
  let tell: (show: AboutShow) => void = () => {};
  render(
    <About
      call={call}
      initial={show("about")}
      subscribe={(onShow) => {
        tell = onShow;
        return () => {};
      }}
      close={() => {}}
    />,
  );
  await screen.findByRole("heading", { name: "Trawler" });
  act(() => tell(show("privacy")));
  expect(await screen.findByRole("heading", { name: "Privacy" })).toBeInTheDocument();
  act(() => tell(show("about", GUIDE)));
  expect(await screen.findByRole("button", { name: "copy address" })).toHaveTextContent(GUIDE);
});

test("Esc closes the window", async () => {
  const { call } = shell();
  const close = vi.fn();
  render(<About call={call} initial={show("about")} subscribe={quiet} close={close} />);
  await screen.findByRole("heading", { name: "Trawler" });
  fireEvent.keyDown(window, { key: "Escape" });
  await waitFor(() => expect(close).toHaveBeenCalledTimes(1));
});

test("↓ on the page names moves to the next page", async () => {
  const { call } = shell();
  render(<About call={call} initial={show("about")} subscribe={quiet} close={() => {}} />);
  await screen.findByRole("heading", { name: "Trawler" });
  fireEvent.keyDown(screen.getByRole("tab", { name: "About" }), { key: "ArrowDown" });
  expect(await screen.findByRole("heading", { name: "Third-party licences" })).toBeInTheDocument();
});

test("the page the shell opened the window on is read from its mark, About otherwise", () => {
  expect(shownAtOpen({ [ABOUT_SHOW_GLOBAL]: { page: "licences", unopened: null } })).toEqual(show("licences"));
  expect(shownAtOpen({ [ABOUT_SHOW_GLOBAL]: { page: "about", unopened: GUIDE } })).toEqual(show("about", GUIDE));
  expect(shownAtOpen({})).toEqual(show("about"));
  expect(shownAtOpen({ [ABOUT_SHOW_GLOBAL]: { page: "elsewhere" } })).toEqual(show("about"));
});
