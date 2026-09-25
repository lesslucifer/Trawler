import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import type { BodyView, FlowView } from "./api";
import { Composer, COMPOSER_COPY } from "./Composer";
import { blankDraft, draftFrom, type Draft } from "./draft";
import type { Outcome } from "./useDraft";

const b64 = (text: string) => btoa(Array.from(new TextEncoder().encode(text), (b) => String.fromCharCode(b)).join(""));

function seed(over: Partial<FlowView> = {}): FlowView {
  return {
    id: "seed-1",
    started_at: "2026-09-25T14:02:11.482+08:00",
    duration_ms: 87,
    method: "POST",
    uri: "https://a.test/users",
    status: 201,
    source: "captured",
    seed: null,
    request_http_version: "HTTP/1.1",
    response_http_version: "HTTP/1.1",
    request_headers: [
      { name: "Host", value: "a.test" },
      { name: "Content-Type", value: "application/json" },
      { name: "Authorization", value: "Bearer OLD" },
      { name: "Content-Length", value: "12" },
      { name: "Proxy-Connection", value: "keep-alive" },
      { name: "Cookie", value: "sid=9f2c" },
    ],
    response_headers: [],
    request_body: { state: "captured", size: 12, data_base64: b64('{"name":"a"}') },
    response_body: null,
    ...over,
  };
}

const SENT: FlowView = { ...seed(), id: "sent-1", source: "sent", seed: "seed-1", status: 201, duration_ms: 142 };

function Harness({
  start,
  outcome = { phase: "idle" },
  onSend = () => {},
  onClose = () => {},
  onOpenFlow = () => {},
}: {
  start: Draft;
  outcome?: Outcome;
  onSend?: () => void;
  onClose?: () => void;
  onOpenFlow?: (id: string) => void;
}) {
  const [draft, setDraft] = useState(start);
  return (
    <Composer
      draft={draft}
      outcome={outcome}
      onEdit={(update) => setDraft((d) => update(d))}
      onSend={onSend}
      onClose={onClose}
      onOpenFlow={onOpenFlow}
    />
  );
}

const rows = () => screen.getAllByTestId("header-row");
const marks = () => screen.getAllByTestId("row-mark").map((m) => m.textContent);
const sendButton = () => screen.getByRole("button", { name: COMPOSER_COPY.send });
const zone = () => screen.getByTestId("send-zone");
const value = (name: string) => screen.getByRole("textbox", { name: `${name} value` });

describe("a draft seeded from a flow", () => {
  it("opens with focus in the URL field, filled from the seed", () => {
    render(<Harness start={draftFrom(seed())} />);

    const url = screen.getByRole("textbox", { name: "url" });
    expect(url).toHaveFocus();
    expect(url).toHaveValue("https://a.test/users");
    expect(url.className).toContain("font-mono");
    expect(screen.getByTestId("provenance")).toHaveTextContent(COMPOSER_COPY.fromFlow);
  });

  it("lists the headers it will send, and names the rows the core rewrites", () => {
    render(<Harness start={draftFrom(seed())} />);

    expect(screen.getByRole("heading", { name: "Headers · 4" })).toBeInTheDocument();
    expect(rows()).toHaveLength(4);
    expect(screen.getByTestId("not-in-draft")).toHaveTextContent(
      "not in the draft: Content-Length — set from the body · Proxy-Connection — the client's to set",
    );
  });

  it("the unedited Host row follows the URL, and shows the URL's host as it changes", () => {
    render(<Harness start={draftFrom(seed())} />);

    expect(marks()).toEqual([COMPOSER_COPY.follows, "", "", ""]);
    fireEvent.change(screen.getByRole("textbox", { name: "url" }), { target: { value: "https://b.test/users" } });
    expect(value("Host")).toHaveValue("b.test");
    expect(screen.getByRole("button", { name: COMPOSER_COPY.startOver })).toBeInTheDocument();
  });

  it("an edited header value marks its row and no neighbour", () => {
    render(<Harness start={draftFrom(seed())} />);

    fireEvent.change(value("Authorization"), { target: { value: "Bearer NEW" } });

    expect(marks()).toEqual([COMPOSER_COPY.follows, "", `${COMPOSER_COPY.edited}${COMPOSER_COPY.revert}`, ""]);
    expect(rows().filter((r) => r.dataset.mark === "edited")).toHaveLength(1);
  });

  it("revert restores the value and puts focus on it", () => {
    render(<Harness start={draftFrom(seed())} />);
    fireEvent.change(value("Authorization"), { target: { value: "Bearer NEW" } });

    fireEvent.click(screen.getByRole("button", { name: COMPOSER_COPY.revert }));

    expect(value("Authorization")).toHaveValue("Bearer OLD");
    expect(value("Authorization")).toHaveFocus();
    expect(screen.queryByRole("button", { name: COMPOSER_COPY.revert })).toBeNull();
  });

  it("removing a row counts it and moves focus to the next row's name", () => {
    render(<Harness start={draftFrom(seed())} />);

    fireEvent.click(screen.getByRole("button", { name: "remove Content-Type" }));

    expect(screen.getByRole("heading", { name: "Headers · 3 · 1 removed" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "header 2 name" })).toHaveValue("Authorization");
    expect(screen.getByRole("textbox", { name: "header 2 name" })).toHaveFocus();
  });

  it("removing the last row moves focus to add header, never to the body", () => {
    render(<Harness start={draftFrom(seed())} />);

    fireEvent.click(screen.getByRole("button", { name: "remove Cookie" }));

    expect(screen.getByRole("button", { name: COMPOSER_COPY.addHeader })).toHaveFocus();
  });

  it("add header appends a row marked added, with focus in its name", () => {
    render(<Harness start={draftFrom(seed())} />);

    fireEvent.click(screen.getByRole("button", { name: COMPOSER_COPY.addHeader }));

    const name = screen.getByRole("textbox", { name: "header 5 name" });
    expect(name).toHaveFocus();
    expect(marks()[4]).toBe(COMPOSER_COPY.added);
  });

  it("start over returns the draft to its seed and focus to the URL", () => {
    render(<Harness start={draftFrom(seed())} />);
    fireEvent.change(value("Authorization"), { target: { value: "Bearer NEW" } });
    fireEvent.click(screen.getByRole("button", { name: "remove Cookie" }));

    fireEvent.click(screen.getByRole("button", { name: COMPOSER_COPY.startOver }));

    expect(value("Authorization")).toHaveValue("Bearer OLD");
    expect(rows()).toHaveLength(4);
    expect(screen.getByRole("textbox", { name: "url" })).toHaveFocus();
    expect(screen.queryByRole("button", { name: COMPOSER_COPY.startOver })).toBeNull();
  });

  it("the body is the seed's text in a multi-line mono box, marked once edited", () => {
    render(<Harness start={draftFrom(seed())} />);
    const body = screen.getByRole("textbox", { name: "body" });
    expect(body.tagName).toBe("TEXTAREA");
    expect(body).toHaveValue('{"name":"a"}');
    expect(body.className).toContain("h-body-max");
    expect(screen.queryByTestId("body-mark")).toBeNull();

    fireEvent.change(body, { target: { value: '{"name":"alice"}' } });
    fireEvent.click(within(screen.getByTestId("body-mark")).getByRole("button", { name: COMPOSER_COPY.revert }));

    expect(body).toHaveValue('{"name":"a"}');
    expect(body).toHaveFocus();
  });
});

describe("a seed whose body the composer cannot show", () => {
  it.each<[BodyView, string]>([
    [{ state: "streaming", content_type: "text/event-stream" }, "streaming (text/event-stream) — passed through, not captured"],
    [{ state: "not_captured", reason: "too_large", actual: 3_355_443 }, "not captured — too large (3.2 MiB)"],
    [{ state: "not_captured", reason: "read_error", actual: null }, "not captured — read error"],
  ])("says the inspector's sentence, and the slot still takes typing", (body, sentence) => {
    render(<Harness start={draftFrom(seed({ request_body: body }))} />);
    const standIn = screen.getByTestId("body-stand-in");
    expect(standIn).toHaveTextContent(sentence);
    expect(standIn).toHaveTextContent(COMPOSER_COPY.typeToSend);

    const box = screen.getByRole("textbox", { name: "body" });
    expect(box).toHaveValue("");
    fireEvent.change(box, { target: { value: "typed" } });

    expect(box).toHaveValue("typed");
    expect(standIn).toHaveTextContent(COMPOSER_COPY.typedInstead);
    expect(screen.queryByTestId("body-mark")).toBeNull();
  });

  it("a binary body is described and sent as captured, never put in the field", () => {
    const bytes = String.fromCharCode(0xff, 0x00, 0x01, 0x02);
    render(<Harness start={draftFrom(seed({ request_body: { state: "captured", size: 4, data_base64: btoa(bytes) } }))} />);

    expect(screen.getByTestId("body-stand-in")).toHaveTextContent("binary body, 4 B — sent as captured");
    expect(screen.getByTestId("body-stand-in")).toHaveTextContent(COMPOSER_COPY.typeToReplace);
    expect(screen.getByRole("textbox", { name: "body" })).toHaveValue("");
  });

  it("an empty seed body is an empty field, not an absence", () => {
    render(<Harness start={draftFrom(seed({ request_body: { state: "empty" } }))} />);

    expect(screen.queryByTestId("body-stand-in")).toBeNull();
    expect(screen.getByRole("textbox", { name: "body" })).toHaveAttribute("placeholder", COMPOSER_COPY.bodyPlaceholder);
  });
});

describe("a draft from scratch", () => {
  it("says new request, marks nothing, and cannot send without a URL", () => {
    render(<Harness start={blankDraft()} />);

    expect(screen.getByTestId("provenance")).toHaveTextContent(COMPOSER_COPY.fromScratch);
    expect(screen.getByRole("button", { name: COMPOSER_COPY.close })).toBeInTheDocument();
    expect(sendButton()).toBeDisabled();
    expect(zone()).toHaveTextContent(COMPOSER_COPY.needsUrl);

    fireEvent.click(screen.getByRole("button", { name: COMPOSER_COPY.addHeader }));
    expect(marks()).toEqual([""]);
  });

  it("a URL enables send", () => {
    render(<Harness start={blankDraft()} />);

    fireEvent.change(screen.getByRole("textbox", { name: "url" }), { target: { value: "https://a.test/" } });

    expect(sendButton()).toBeEnabled();
    expect(zone()).toHaveTextContent(COMPOSER_COPY.notSent);
  });
});

describe("the send zone", () => {
  it("send calls onSend", () => {
    const onSend = vi.fn();
    render(<Harness start={draftFrom(seed())} onSend={onSend} />);

    fireEvent.click(sendButton());

    expect(onSend).toHaveBeenCalledOnce();
  });

  it.each<Outcome>([
    { phase: "idle" },
    { phase: "sending" },
    { phase: "sent", flow: SENT },
    { phase: "failed", reason: "no response within 30 s" },
  ])("send is disabled exactly when sending… shows ($phase)", (outcome) => {
    render(<Harness start={draftFrom(seed())} outcome={outcome} />);

    const sending = zone().textContent?.includes(COMPOSER_COPY.sending) ?? false;
    expect(sendButton().hasAttribute("disabled")).toBe(sending);
    expect(sending).toBe(outcome.phase === "sending");
  });

  it("a sent answer is the pill, the duration and a way to the new flow, with no ✓", () => {
    const onOpenFlow = vi.fn();
    render(<Harness start={draftFrom(seed())} outcome={{ phase: "sent", flow: SENT }} onOpenFlow={onOpenFlow} />);

    expect(within(zone()).getByLabelText(/^201\b/)).toBeInTheDocument();
    expect(zone()).toHaveTextContent("142 ms");
    expect(zone()).not.toHaveTextContent("✓");
    fireEvent.click(screen.getByRole("button", { name: COMPOSER_COPY.openNewFlow }));
    expect(onOpenFlow).toHaveBeenCalledWith("sent-1");
  });

  it("a failed send is ⚠ and the core's sentence verbatim, with no pill and no duration", () => {
    const reason = "send failed: tcp connect error: Connection refused (os error 61)";
    render(<Harness start={draftFrom(seed())} outcome={{ phase: "failed", reason }} />);

    const failed = screen.getByTestId("send-failed");
    expect(failed).toHaveTextContent(`⚠${reason}`);
    expect(within(zone()).queryByLabelText(/^\d{3}\b/)).toBeNull();
    expect(zone()).not.toHaveTextContent(/\d+ ms/);
  });

  it("focus returns to send when the answer lands and the disabled button had dropped it", () => {
    const { rerender } = render(<Harness start={draftFrom(seed())} outcome={{ phase: "sending" }} />);
    (document.activeElement as HTMLElement | null)?.blur();
    expect(document.activeElement).toBe(document.body);

    rerender(<Harness start={draftFrom(seed())} outcome={{ phase: "failed", reason: "no" }} />);

    expect(sendButton()).toHaveFocus();
  });

  it("an answer landing never pulls focus out of a field the user is typing in", () => {
    const { rerender } = render(<Harness start={draftFrom(seed())} outcome={{ phase: "sending" }} />);
    screen.getByRole("textbox", { name: "body" }).focus();

    rerender(<Harness start={draftFrom(seed())} outcome={{ phase: "sent", flow: SENT }} />);

    expect(screen.getByRole("textbox", { name: "body" })).toHaveFocus();
  });
});

describe("leaving", () => {
  it("× and Esc both dismiss", () => {
    const onClose = vi.fn();
    render(<Harness start={draftFrom(seed())} onClose={onClose} />);

    fireEvent.click(screen.getByRole("button", { name: COMPOSER_COPY.backToFlow }));
    fireEvent.keyDown(document, { key: "Escape" });

    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("Esc that closes the method picker does not also dismiss the composer", () => {
    const onClose = vi.fn();
    render(<Harness start={draftFrom(seed())} onClose={onClose} />);
    const picker = screen.getByRole("button", { name: "method" });

    fireEvent.click(picker);
    fireEvent.keyDown(picker, { key: "Escape" });

    expect(onClose).not.toHaveBeenCalled();
  });
});
