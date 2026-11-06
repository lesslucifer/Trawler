import type { FlowView, Leg } from "./api";
import { HEADERS_LABEL, URI_LABEL } from "./contentKind";
import type { DataPayload, Heading } from "./dataWindow";

export function flowCaption(flow: FlowView, what: string): Pick<DataPayload, "title" | "heading"> {
  const where = flow.uri.replace(/^[a-z][a-z0-9+.-]*:\/\//i, "");
  const [, host, path, query] = /^([^/?#]*)([^?#]*)(.*)$/s.exec(where)!;
  const heading: Heading = { what, method: flow.method, host, path, query };
  return { title: `${what} · ${flow.method} ${where}`, heading };
}

export function uriPayload(flow: FlowView): DataPayload {
  return { key: `${flow.id}:uri`, ...flowCaption(flow, "uri"), bytes: new TextEncoder().encode(flow.uri), typeHint: URI_LABEL };
}

export function headersPayload(flow: FlowView, leg: Leg): DataPayload {
  const headers = leg === "request" ? flow.request_headers : flow.response_headers;
  const lines = headers.map(({ name, value }) => `${name}: ${value}`).join("\n");
  return {
    key: `${flow.id}:${leg}:headers`,
    ...flowCaption(flow, `${leg} headers`),
    bytes: new TextEncoder().encode(lines),
    typeHint: HEADERS_LABEL,
  };
}
