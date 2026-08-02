import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { ImageRenderer } from "./ImageRenderer";

const BYTES = new Uint8Array(2048).fill(0x89);

beforeEach(() => {
  vi.stubGlobal("URL", {
    ...URL,
    createObjectURL: vi.fn(() => "blob:body"),
    revokeObjectURL: vi.fn(),
  });
});

describe("ImageRenderer", () => {
  it("renders the bytes as an image from an object URL", () => {
    render(<ImageRenderer bytes={BYTES} contentType="image/png" />);
    expect(screen.getByTestId("body-image")).toHaveAttribute("src", "blob:body");
  });

  it("hands the served content type to the Blob, so the browser decodes it correctly", () => {
    render(<ImageRenderer bytes={BYTES} contentType="image/webp" />);

    const [blob] = vi.mocked(URL.createObjectURL).mock.calls[0];
    expect((blob as Blob).type).toBe("image/webp");
  });

  it("captions the size before the image has loaded", () => {
    render(<ImageRenderer bytes={BYTES} contentType="image/png" />);
    expect(screen.getByText("image/png · 2.0 KiB")).toBeInTheDocument();
  });

  it("adds the intrinsic dimensions once the image reports them", () => {
    render(<ImageRenderer bytes={BYTES} contentType="image/png" />);

    const img = screen.getByTestId("body-image");
    Object.defineProperty(img, "naturalWidth", { value: 128 });
    Object.defineProperty(img, "naturalHeight", { value: 64 });
    fireEvent.load(img);

    expect(screen.getByText("128 × 64 · image/png · 2.0 KiB")).toBeInTheDocument();
  });

  it("captions in UI text, not mono — it is metadata, not payload", () => {
    render(<ImageRenderer bytes={BYTES} contentType="image/png" />);

    const caption = screen.getByText(/2.0 KiB/);
    expect(caption.className).toContain("font-ui");
    expect(caption.className).not.toContain("font-mono");
  });

  it("letterboxes rather than stretching, over a checkerboard that reveals transparency", () => {
    render(<ImageRenderer bytes={BYTES} contentType="image/png" />);

    const img = screen.getByTestId("body-image");
    expect(img.className).toContain("max-w-full");
    expect(img.style.backgroundImage).toContain("repeating-conic-gradient");
    expect(img.style.backgroundSize).toBe("12px 12px");
  });

  it("shows no cut marker — the element renders every byte, and save serves the rest", () => {
    render(<ImageRenderer bytes={new Uint8Array(5_000_000)} contentType="image/png" />);
    expect(screen.queryByText(/showing/)).not.toBeInTheDocument();
  });

  it("releases the object URL when it unmounts", () => {
    const { unmount } = render(<ImageRenderer bytes={BYTES} contentType="image/png" />);
    unmount();

    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:body");
  });

  it("survives a null content type — the Blob just gets no MIME hint", () => {
    render(<ImageRenderer bytes={BYTES} contentType={null} />);
    expect(screen.getByText("2.0 KiB")).toBeInTheDocument();
  });
});
