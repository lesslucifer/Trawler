import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { vi } from "vitest";
import { FlowDetail } from "./FlowDetail";
import type { FlowView, TrawlerApi } from "./api";

const stubApi = (): TrawlerApi => ({
  listFlows: vi.fn(),
  connectionCount: vi.fn(async () => 0),
  caCertPem: vi.fn(async () => ""),
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

// The pane holds no leg state, so the panel is what puts both legs on the page
// at once — the contract that replaced the leg switch the pane used to own.
test("mounts one body pane per leg, each under its own heading", async () => {
  const api = stubApi();
  render(
    <FlowDetail
      api={api}
      flow={view({ request_body: { state: "captured", size: 5, data_base64: "" } })}
      onClose={() => {}}
    />,
  );

  expect(screen.getByText("Request body")).toBeInTheDocument();
  expect(screen.getByText("Response body")).toBeInTheDocument();
  await waitFor(() => expect(screen.getAllByTestId("body-text")).toHaveLength(2));
  expect(api.getBody).toHaveBeenCalledWith("f1", "request", true);
  expect(api.getBody).toHaveBeenCalledWith("f1", "response", true);
});

test("each pane owns its own mode — switching one leg to raw leaves the other alone", async () => {
  const api = stubApi();
  render(
    <FlowDetail
      api={api}
      flow={view({ request_body: { state: "captured", size: 5, data_base64: "" } })}
      onClose={() => {}}
    />,
  );
  await waitFor(() => expect(screen.getAllByTestId("body-text")).toHaveLength(2));

  fireEvent.click(screen.getByRole("radiogroup", { name: "request body mode" })
    .querySelector("[role='radio'][aria-checked='false']") as HTMLElement);

  await waitFor(() => expect(api.getBody).toHaveBeenCalledWith("f1", "request", false));
  expect(api.getBody).not.toHaveBeenCalledWith("f1", "response", false);
});

test("close button fires onClose", () => {
  const onClose = vi.fn();
  render(<FlowDetail api={stubApi()} flow={view()} onClose={onClose} />);
  fireEvent.click(screen.getByLabelText("close"));
  expect(onClose).toHaveBeenCalled();
});
