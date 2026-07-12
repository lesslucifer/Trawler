import { render, screen, waitFor, fireEvent, act } from "@testing-library/react";
import { afterEach, beforeAll, vi } from "vitest";
import App from "./App";
import type { Filter, FlowSummary, StreamHandlers, TrawlerApi } from "./api";

// jsdom reports 0 layout, so react-virtual would render no rows. Give the scroll
// container a real height/width so the virtualizer materializes rows.
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, value: 600 });
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, value: 900 });
  HTMLElement.prototype.getBoundingClientRect = () =>
    ({ height: 600, width: 900, top: 0, left: 0, bottom: 600, right: 900, x: 0, y: 0, toJSON() {} }) as DOMRect;
});

function flow(over: Partial<FlowSummary> = {}): FlowSummary {
  return {
    id: over.id ?? "f1",
    started_at: "2026-07-12T10:00:00Z",
    duration_ms: 5,
    method: "GET",
    uri: "http://example.com/",
    status: 200,
    request_body_state: "empty",
    response_body_state: "empty",
    ...over,
  };
}

// Mock API: `store` is the current server-side result; a filter with host !== ""
// narrows it (mirroring GET /flows). `fire()` triggers a captured SSE listener;
// `setLive(b)` drives the connection-state callback the real EventSource wires.
function mockApi(store: () => FlowSummary[]) {
  let onEvent = () => {};
  let onLive: (live: boolean) => void = () => {};
  const api: TrawlerApi = {
    listFlows: vi.fn(async (filter: Filter) => {
      const rows = store();
      if (filter.host?.trim()) {
        return rows.filter((f) => f.uri.includes(filter.host!.trim()));
      }
      return rows;
    }),
    stream: vi.fn((handlers: StreamHandlers) => {
      onEvent = handlers.onEvent;
      onLive = handlers.onLive ?? (() => {});
      return () => {};
    }),
  };
  return {
    api,
    fire: () => act(() => onEvent()),
    setLive: (live: boolean) => act(() => onLive(live)),
  };
}

afterEach(() => vi.restoreAllMocks());

test("renders a flow row from the API and applies status tone", async () => {
  const { api } = mockApi(() => [flow()]);
  render(<App api={api} />);
  const row = await screen.findByTestId("flow-row");
  expect(row).toHaveTextContent("example.com");
  expect(row.querySelector(".text-ok")).not.toBeNull();
});

test("a non-matching host filter empties the list; SSE appends a new row", async () => {
  let rows = [flow({ id: "f1", uri: "http://example.com/" })];
  const { api, fire } = mockApi(() => rows);
  render(<App api={api} />);
  await screen.findByTestId("flow-row");

  fireEvent.change(screen.getByLabelText("host"), { target: { value: "other.test" } });
  await waitFor(() => expect(screen.queryByTestId("flow-row")).toBeNull());
  expect(screen.getByTestId("empty")).toBeInTheDocument();

  fireEvent.change(screen.getByLabelText("host"), { target: { value: "" } });
  rows = [...rows, flow({ id: "f2", uri: "http://example.com/second", status: 404 })];
  fire();
  await waitFor(() => expect(screen.getAllByTestId("flow-row")).toHaveLength(2));
});

test("the live dot reflects the real stream connection state", async () => {
  const { api, setLive } = mockApi(() => []);
  render(<App api={api} />);
  // Starts disconnected until the stream opens — not eagerly green.
  expect(screen.getByTestId("live").className).toMatch(/bg-warn/);
  setLive(true);
  await waitFor(() => expect(screen.getByTestId("live").className).toMatch(/bg-ok/));
  setLive(false);
  await waitFor(() => expect(screen.getByTestId("live").className).toMatch(/bg-warn/));
});
