import type { FlowSummary } from "../src/api";

// Every fixture is a pure function of its index, so two runs of the suite differ
// in timing and in nothing else — a benchmark whose input varies measures the
// input.
const HOSTS = [
  "api.example.com",
  "cdn.example.com:8443",
  "auth.example.org",
  "telemetry.internal",
  "static.example.net",
];

const METHODS = ["GET", "POST", "PUT", "DELETE"];

const BASE_MS = Date.UTC(2026, 8, 6, 9, 0, 0);

export function flows(count: number): FlowSummary[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `flow-${i}`,
    started_at: new Date(BASE_MS + i * 137).toISOString(),
    duration_ms: (i * 31) % 900,
    method: METHODS[i % METHODS.length],
    uri: `https://${HOSTS[i % HOSTS.length]}/v2/resource/${i}/detail?page=${i % 20}&expand=all`,
    status: [200, 201, 304, 404, 500][i % 5],
    request_body_state: "empty",
    response_body_state: "captured",
  }));
}

// The case a host memo cannot help: every flow names a host no other flow names,
// so nothing is ever a repeat. A real capture is nothing like this — it is the
// bound on what remembering a parse can be worth.
export function flowsAcrossDistinctHosts(count: number): FlowSummary[] {
  return flows(count).map((flow, i) => ({
    ...flow,
    uri: `https://host-${i}.example.com/v2/resource/${i}/detail?page=${i % 20}&expand=all`,
  }));
}

// A JSON-ish text body of roughly `bytes` UTF-8 bytes, mixed ASCII and
// multi-byte so a byte count is never just the character count.
export function textBody(bytes: number): string {
  const unit = `{"id":"a1b2c3","note":"résumé — naïve","payload":"${"x".repeat(64)}"},\n`;
  const unitBytes = new TextEncoder().encode(unit).length;
  return unit.repeat(Math.ceil(bytes / unitBytes));
}

export function formBody(pairs: number): string {
  return Array.from({ length: pairs }, (_, i) => `field${i}=value%20${i}%C3%A9`).join("&");
}
