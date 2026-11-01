import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { StepMedia, type StepMediaSource } from "./StepMedia";

const VIDEO: StepMediaSource = { kind: "video", src: { mp4: "/a.mp4", webm: "/a.webm" } };
const IMAGE: StepMediaSource = { kind: "image", src: "/a.png" };
const ALT = "Keychain Access: Trawler Proxy CA → Trust → Always Trust";

const reducedMotion = (reduce: boolean) =>
  vi.spyOn(window, "matchMedia").mockImplementation(
    (query) => ({ matches: reduce && query.includes("reduce") }) as MediaQueryList,
  );

afterEach(() => vi.restoreAllMocks());

describe("StepMedia", () => {
  it("names the path it shows as its accessible name, and the OS version under it", () => {
    render(<StepMedia media={VIDEO} alt={ALT} shows="macOS 26" />);

    expect(screen.getByLabelText(ALT).tagName).toBe("VIDEO");
    expect(screen.getByText("macOS 26").tagName).toBe("FIGCAPTION");
  });

  // W18: a phone's portrait recording stands narrow rather than a screen tall.
  it.each([
    ["video", VIDEO],
    ["image", IMAGE],
  ] as const)("a %s keeps its aspect within the pane's width and the step-media height", (_kind, media) => {
    const { container } = render(<StepMedia media={media} alt={ALT} shows="iOS 26.5" />);

    expect(container.querySelector("video, img")!.className.split(" ")).toEqual(
      expect.arrayContaining(["w-auto", "max-w-full", "max-h-step-media"]),
    );
  });

  it("an image carries the path as its alt text", () => {
    render(<StepMedia media={IMAGE} alt={ALT} shows="macOS 26" />);

    expect(screen.getByAltText(ALT)).toHaveAttribute("src", "/a.png");
  });

  it("a video ships two sources, so every webview plays one, and can always be paused", () => {
    render(<StepMedia media={VIDEO} alt={ALT} shows="macOS 26" />);

    const video = screen.getByLabelText(ALT) as HTMLVideoElement;
    expect([...video.querySelectorAll("source")].map((s) => s.getAttribute("type"))).toEqual([
      "video/mp4",
      "video/webm",
    ]);
    expect(video.controls).toBe(true);
    expect(video.muted).toBe(true);
    expect(video.loop).toBe(true);
  });

  it("plays on its own unless the system asks for reduced motion", () => {
    reducedMotion(false);
    const { unmount } = render(<StepMedia media={VIDEO} alt={ALT} shows="macOS 26" />);
    expect((screen.getByLabelText(ALT) as HTMLVideoElement).autoplay).toBe(true);
    unmount();

    reducedMotion(true);
    render(<StepMedia media={VIDEO} alt={ALT} shows="macOS 26" />);
    expect((screen.getByLabelText(ALT) as HTMLVideoElement).autoplay).toBe(false);
  });

  it("a file that does not load leaves nothing behind, caption included — the sentence is the step", () => {
    const { container } = render(<StepMedia media={VIDEO} alt={ALT} shows="macOS 26" />);

    fireEvent.error(container.querySelector('source[type="video/webm"]')!);
    expect(container).toBeEmptyDOMElement();
  });

  it("an image that does not load leaves nothing behind", () => {
    const { container } = render(<StepMedia media={IMAGE} alt={ALT} shows="macOS 26" />);

    fireEvent.error(screen.getByAltText(ALT));
    expect(container).toBeEmptyDOMElement();
  });
});
