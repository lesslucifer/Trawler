import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { TEXT_MAX } from "../body";
import { XmlRenderer } from "./XmlRenderer";

const PRETTY = '<order id="8f3a91c2">\n  <status>pending</status>\n</order>';

describe("XmlRenderer", () => {
  it("renders the server's indented source in the editor surface", () => {
    render(<XmlRenderer text={PRETTY} />);

    expect(screen.getByTestId("body-xml")).toBeInTheDocument();
    expect(screen.getByTestId("body-xml").textContent).toContain("<status>pending</status>");
  });

  it("keeps the document exact — tags, attributes and text unaltered", () => {
    const { container } = render(<XmlRenderer text={PRETTY} />);

    const shown = container.querySelector(".cm-content")?.textContent ?? "";
    expect(shown).toContain('<order id="8f3a91c2">');
    expect(shown).toContain("</order>");
  });

  it("folds by element — the round-2 want, served by the shared surface", () => {
    const { container } = render(<XmlRenderer text={PRETTY} />);
    expect(container.querySelector(".cm-foldGutter")).toBeInTheDocument();
  });

  it("renders an HTML body down the same path", () => {
    render(<XmlRenderer text={"<div>\n  <p>Order confirmed</p>\n</div>"} />);
    expect(screen.getByTestId("body-xml").textContent).toContain("<p>Order confirmed</p>");
  });

  it("falls back to text with a cut marker over the cap", () => {
    render(<XmlRenderer text={`<a>${"x".repeat(TEXT_MAX)}</a>`} />);

    expect(screen.getByTestId("body-text")).toBeInTheDocument();
    expect(screen.getByText(/⚠ showing .* — save to see all/)).toBeInTheDocument();
  });
});
