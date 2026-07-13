import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { vi } from "vitest";
import { FlowDetail } from "./FlowDetail";
import type { FlowView, TrawlerApi } from "./api";

const stubApi = (): TrawlerApi => ({
  listFlows: vi.fn(),
  getFlow: vi.fn(),
  getBody: vi.fn(async () => ({ bytes: new TextEncoder().encode("body!"), contentType: "text/plain" })),
  stream: vi.fn(),
});

function view(over: Partial<FlowView> = {}): FlowView {
  return {
    id: "f1",
    started_at: "2026-07-12T10:00:00Z",
    duration_ms: 42,
    method: "GET",
    uri: "http://example.com/a?x=1",
    status: 200,
    request_http_version: "HTTP/1.1",
    response_http_version: "HTTP/2.0",
    request_headers: [{ name: "Accept", value: "application/json" }],
    response_headers: [{ name: "Content-Type", value: "application/json" }],
    request_body: { state: "empty" },
    response_body: { state: "captured", size: 12, data_base64: "" },
    ...over,
  };
}

test("shows a loading notice until the flow arrives", () => {
  render(<FlowDetail api={stubApi()} flow={null} onClose={() => {}} />);
  expect(screen.getByTestId("detail-loading")).toBeInTheDocument();
});

test("renders request line, status, timing, versions, headers, and curl", () => {
  render(<FlowDetail api={stubApi()} flow={view()} onClose={() => {}} />);
  expect(screen.getByTestId("detail")).toHaveTextContent("GET");
  expect(screen.getByText("200").className).toMatch(/text-ok/);
  expect(screen.getByTestId("detail")).toHaveTextContent("42 ms");
  expect(screen.getByTestId("detail")).toHaveTextContent("HTTP/1.1 → HTTP/2.0");
  expect(screen.getByText("Accept")).toBeInTheDocument();
  expect(screen.getByText("Content-Type")).toBeInTheDocument();
  expect(screen.getByTestId("curl")).toHaveTextContent(
    "curl 'http://example.com/a?x=1' -H 'Accept: application/json'",
  );
});

test("copy button writes the curl to the clipboard", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  render(<FlowDetail api={stubApi()} flow={view()} onClose={() => {}} />);
  fireEvent.click(screen.getByText("copy"));
  expect(writeText).toHaveBeenCalledWith(
    "curl 'http://example.com/a?x=1' -H 'Accept: application/json'",
  );
  await waitFor(() => expect(screen.getByText("copied")).toBeInTheDocument());
  vi.unstubAllGlobals();
});

test("close button fires onClose", () => {
  const onClose = vi.fn();
  render(<FlowDetail api={stubApi()} flow={view()} onClose={onClose} />);
  fireEvent.click(screen.getByLabelText("close"));
  expect(onClose).toHaveBeenCalled();
});
