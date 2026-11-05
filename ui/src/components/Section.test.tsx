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

  it("className joins the section element in both variants, so a host can size the zone", () => {
    const { container } = render(
      <>
        <Section title="a" className="flex-1 min-h-0">x</Section>
        <Section title="b" variant="band" className="shrink-0">y</Section>
      </>,
    );

    const [plain, band] = [...container.querySelectorAll("section")];
    expect(plain.className).toContain("flex-1");
    expect(plain.className).toContain("border-b");
    expect(band.className).toContain("shrink-0");
  });

  it("an action sits at the heading's trailing edge, in the heading's row, and the heading stays on one line", () => {
    render(
      <Section title="Headers · 3" action={<button type="button">open all</button>}>
        <p>rows</p>
      </Section>,
    );

    const heading = screen.getByRole("heading", { name: "Headers · 3", level: 2 });
    const action = screen.getByRole("button", { name: "open all" });
    expect(heading.parentElement).toBe(action.parentElement!.parentElement);
    expect(heading.className).toContain("whitespace-nowrap");
    expect(action.parentElement!.className).toContain("ml-auto");
    expect(action.parentElement!.className).toContain("shrink-0");
  });

  it("no action, no row: the heading is the section's first child as before", () => {
    const { container } = render(<Section title="Body" action={false}>x</Section>);

    expect(container.querySelector("section > h2")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});

