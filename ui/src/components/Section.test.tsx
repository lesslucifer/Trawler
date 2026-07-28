import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { Section } from "./Section";

describe("Section", () => {
  it("renders a section landmark with an h2 heading and the content verbatim", () => {
    const { container } = render(
      <Section title="request">
        <p>body</p>
      </Section>,
    );

    const heading = screen.getByRole("heading", { name: "request", level: 2 });
    expect(heading.className).toContain("uppercase");
    expect(heading.className).toContain("text-ink-mute");
    expect(container.querySelector("section")).toBeInTheDocument();
    expect(screen.getByText("body")).toBeInTheDocument();
  });

  it("carries the hairline rhythm and panel padding", () => {
    const { container } = render(<Section title="request">x</Section>);

    const section = container.querySelector("section");
    expect(section?.className).toContain("border-b");
    expect(section?.className).toContain("border-hairline");
    expect(section?.className).toContain("px-4");
    expect(section?.className).toContain("py-3");
  });

  it("variant=band labels a group at h2 and demotes its members to h3 (F3)", () => {
    render(
      <Section title="response" variant="band">
        <Section title="headers">a</Section>
        <Section title="body">b</Section>
      </Section>,
    );

    expect(screen.getByRole("heading", { name: "response", level: 2 })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "headers", level: 3 })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "body", level: 3 })).toBeInTheDocument();
  });

  it("the band label sits one ink step above its members", () => {
    render(
      <Section title="response" variant="band">
        <Section title="headers">a</Section>
      </Section>,
    );

    expect(screen.getByRole("heading", { level: 2 }).className).toContain("text-ink-strong");
    expect(screen.getByRole("heading", { level: 3 }).className).toContain("text-ink-mute");
  });

  it("the band's group slot adds no padding of its own, so member padding never doubles", () => {
    const { container } = render(
      <Section title="response" variant="band">
        <Section title="headers">a</Section>
      </Section>,
    );

    const band = container.querySelector('section[data-variant="band"]');
    expect(band?.className ?? "").not.toContain("px-4");
    expect(band?.className ?? "").not.toContain("py-3");
  });

  it("a Section outside any band is always h2 — the level derives from context, never a prop", () => {
    render(
      <>
        <Section title="one">a</Section>
        <Section title="two">b</Section>
      </>,
    );

    expect(screen.getAllByRole("heading", { level: 2 })).toHaveLength(2);
    expect(screen.queryByRole("heading", { level: 3 })).not.toBeInTheDocument();
  });
});
