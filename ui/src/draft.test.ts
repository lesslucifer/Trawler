import { describe, expect, it } from "vitest";
import type { BodyView, FlowView } from "./api";
import { TEXT_MAX } from "./body";
import sender from "../../src/sender.rs?raw";
import {
  addRow,
  binarySentence,
  blankDraft,
  bodyReplaced,
  bodyRevertible,
  canSend,
  coreOwned,
  CORE_OWNED_NAMES,
  draftFrom,
  droppedRows,
  isEdited,
  nextKey,
  removable,
  removedCount,
  removeRow,
  restart,
  revertBody,
  revertRow,
  rowMark,
  seedBody,
  shownValue,
  toRequest,
  updateRow,
  type Draft,
} from "./draft";

const b64 = (text: string) => btoa(Array.from(new TextEncoder().encode(text), (b) => String.fromCharCode(b)).join(""));
const captured = (text: string): BodyView => ({ state: "captured", size: text.length, data_base64: b64(text) });

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
    request_body: captured('{"name":"a"}'),
    response_body: null,
    ...over,
  };
}

const withRow = (draft: Draft) => ({ draft: addRow(draft), key: nextKey(draft) });

const rowNamed = (draft: Draft, name: string) => {
  const row = draft.rows.find((r) => r.name === name);
  if (!row) throw new Error(`no ${name} row`);
  return row;
};

describe("a draft from a seed", () => {
  it("carries the method, the URI and the headers in wire order, minus the rows the core rewrites", () => {
    const draft = draftFrom(seed());

    expect(draft.method).toBe("POST");
    expect(draft.url).toBe("https://a.test/users");
    expect(draft.rows.map((r) => r.name)).toEqual(["Host", "Content-Type", "Authorization", "Cookie"]);
    expect(draft.body).toBe('{"name":"a"}');
  });

  it("names the dropped rows the seed carried, each with why", () => {
    expect(droppedRows(seed()).map((h) => [h.name, coreOwned(h.name)])).toEqual([
      ["Content-Length", "set from the body"],
      ["Proxy-Connection", "the client's to set"],
    ]);
  });

  it.each(["content-length", "Connection", "keep-alive", "PROXY-CONNECTION", "transfer-encoding", "upgrade", "te"])(
    "%s is the core's, whatever its case",
    (name) => {
      expect(coreOwned(name)).toBeDefined();
    },
  );

  it("names exactly the headers the core's sender rewrites — one list, read off src/sender.rs", () => {
    const list = /const CONNECTION_HEADERS: &\[&str\] =\s*&\[([^\]]*)\]/.exec(sender);
    expect(list).not.toBeNull();
    const connection = [...list![1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
    expect(sender).toContain("headers.remove(header::CONTENT_LENGTH)");

    expect([...CORE_OWNED_NAMES].sort()).toEqual(["content-length", ...connection].sort());
    for (const name of connection) expect(coreOwned(name)).toBe("the client's to set");
  });

  it("leaves every other header the draft's", () => {
    expect(coreOwned("Host")).toBeUndefined();
    expect(coreOwned("Cookie")).toBeUndefined();
  });

  it("an unchanged draft marks only the Host row, and sends nothing but the seed", () => {
    const draft = draftFrom(seed());

    expect(draft.rows.map((r) => rowMark(draft, r))).toEqual(["follows", null, null, null]);
    expect(isEdited(draft)).toBe(false);
    expect(toRequest(draft)).toEqual({ seedId: "seed-1", fields: {} });
  });
});

describe("the row marks", () => {
  it("one edited value marks that row and no neighbour", () => {
    const start = draftFrom(seed());
    const draft = updateRow(start, rowNamed(start, "Authorization").key, { value: "Bearer NEW" });

    expect(draft.rows.map((r) => rowMark(draft, r))).toEqual(["follows", null, "edited", null]);
  });

  it("a renamed header is edited too", () => {
    const start = draftFrom(seed());
    const draft = updateRow(start, rowNamed(start, "Cookie").key, { name: "X-Cookie" });

    expect(rowMark(draft, rowNamed(draft, "X-Cookie"))).toBe("edited");
  });

  it("an edit typed back to the seed's value is no edit", () => {
    const start = draftFrom(seed());
    const key = rowNamed(start, "Authorization").key;
    const draft = updateRow(updateRow(start, key, { value: "x" }), key, { value: "Bearer OLD" });

    expect(rowMark(draft, rowNamed(draft, "Authorization"))).toBeNull();
    expect(isEdited(draft)).toBe(false);
  });

  it("an added row is added, with a key no seed row holds", () => {
    const { draft, key } = withRow(draftFrom(seed()));

    expect(draft.rows.filter((r) => r.key === key)).toHaveLength(1);
    expect(key).toBeGreaterThanOrEqual(seed().request_headers.length);
    expect(rowMark(draft, draft.rows[draft.rows.length - 1])).toBe("added");
  });

  it("two added rows never share a key", () => {
    const first = withRow(draftFrom(seed()));
    const second = withRow(first.draft);

    expect(second.key).not.toBe(first.key);
  });

  it("a removed row has nowhere to carry a mark, so the count says it", () => {
    const start = draftFrom(seed());
    const draft = removeRow(start, rowNamed(start, "Cookie").key);

    expect(draft.rows.map((r) => r.name)).not.toContain("Cookie");
    expect(removedCount(draft)).toBe(1);
    expect(isEdited(draft)).toBe(true);
  });

  it("the rows the core rewrites never count as removed", () => {
    expect(removedCount(draftFrom(seed()))).toBe(0);
  });

  it("revert restores the seed's name and value", () => {
    const start = draftFrom(seed());
    const key = rowNamed(start, "Authorization").key;
    const draft = revertRow(updateRow(start, key, { name: "Auth", value: "x" }), key);

    expect(draft.rows.find((r) => r.key === key)).toMatchObject({ name: "Authorization", value: "Bearer OLD" });
  });

  it("revert on an added row changes nothing", () => {
    const { draft, key } = withRow(draftFrom(seed()));

    expect(revertRow(draft, key)).toBe(draft);
  });

  it("a draft from scratch marks nothing", () => {
    const { draft } = withRow(blankDraft());

    expect(rowMark(draft, draft.rows[0])).toBeNull();
    expect(removedCount(draft)).toBe(0);
  });
});

describe("the Host row follows the URL until someone types in it", () => {
  it("the seed's Host row cannot be removed, edited or not; every other row can", () => {
    const start = draftFrom(seed());
    const edited = updateRow(start, rowNamed(start, "Host").key, { value: "c.test" });

    expect(start.rows.map((r) => removable(start, r))).toEqual([false, true, true, true]);
    expect(removable(edited, rowNamed(edited, "Host"))).toBe(false);
  });

  it("a Host row the draft added is its own, and can go", () => {
    const { draft, key } = withRow(blankDraft());
    const named = updateRow(draft, key, { name: "Host" });

    expect(removable(named, named.rows[0])).toBe(true);
  });

  it("shows the URL's host, not the seed's", () => {
    const draft = { ...draftFrom(seed()), url: "https://b.test:8443/users" };

    expect(shownValue(draft, rowNamed(draft, "Host"))).toBe("b.test:8443");
  });

  it("shows nothing for a URL that is not one yet", () => {
    const draft = { ...draftFrom(seed()), url: "htp:/x" };

    expect(shownValue(draft, rowNamed(draft, "Host"))).toBe("");
  });

  it("goes by omission from a changed list, so a new URL carries its own host", () => {
    const start = { ...draftFrom(seed()), url: "https://b.test/users" };
    const draft = updateRow(start, rowNamed(start, "Authorization").key, { value: "Bearer NEW" });

    expect(toRequest(draft)).toEqual({
      seedId: "seed-1",
      fields: {
        url: "https://b.test/users",
        headers: [
          { name: "Content-Type", value: "application/json" },
          { name: "Authorization", value: "Bearer NEW" },
          { name: "Cookie", value: "sid=9f2c" },
        ],
      },
    });
  });

  it("once typed in, it is edited, shown as typed, and sent as typed", () => {
    const start = draftFrom(seed());
    const draft = updateRow(start, rowNamed(start, "Host").key, { value: "c.test" });
    const host = rowNamed(draft, "Host");

    expect(rowMark(draft, host)).toBe("edited");
    expect(shownValue(draft, host)).toBe("c.test");
    expect(toRequest(draft).fields.headers).toContainEqual({ name: "Host", value: "c.test" });
  });

  it("a changed URL alone sends no header list, and the core drops the seed's Host", () => {
    const draft = { ...draftFrom(seed()), url: "https://b.test/users" };

    expect(toRequest(draft)).toEqual({ seedId: "seed-1", fields: { url: "https://b.test/users" } });
  });
});

describe("the body", () => {
  it("captured UTF-8 text fills the field", () => {
    expect(seedBody(captured("héllo\n"))).toEqual({ kind: "text", text: "héllo\n" });
  });

  it("captured and empty is an empty field, and an empty state is the same", () => {
    expect(seedBody(captured(""))).toEqual({ kind: "empty" });
    expect(seedBody({ state: "empty" })).toEqual({ kind: "empty" });
  });

  it("bytes that are not text never enter the field", () => {
    const bytes = String.fromCharCode(0xff, 0xfe, 0x00, 0x01);
    expect(seedBody({ state: "captured", size: 4, data_base64: btoa(bytes) })).toEqual({ kind: "binary", size: 4 });
  });

  it("text over TEXT_MAX never enters the field either", () => {
    expect(seedBody(captured("a".repeat(TEXT_MAX + 1)))).toEqual({ kind: "binary", size: TEXT_MAX + 1 });
    expect(seedBody(captured("a".repeat(TEXT_MAX)))).toMatchObject({ kind: "text" });
  });

  it("is decoded once per body, not once per render", () => {
    const body = captured('{"name":"a"}');

    expect(seedBody(body)).toBe(seedBody(body));
    expect(seedBody(captured('{"name":"a"}'))).not.toBe(seedBody(body));
  });

  it("a binary body is described with its size", () => {
    expect(binarySentence(3277)).toBe("binary body, 3.2 KiB — sent as captured");
  });

  it.each<[BodyView, string]>([
    [{ state: "streaming", content_type: "text/event-stream" }, "streaming (text/event-stream) — passed through, not captured"],
    [{ state: "not_captured", reason: "too_large", actual: 3_355_443 }, "not captured — too large (3.2 MiB)"],
    [{ state: "not_captured", reason: "read_error", actual: null }, "not captured — read error"],
  ])("an absence says the inspector's own sentence", (body, sentence) => {
    expect(seedBody(body)).toEqual({ kind: "absent", sentence });
    expect(draftFrom(seed({ request_body: body })).body).toBe("");
  });

  it("an unedited text body goes by omission, and a changed one goes as typed", () => {
    const draft = draftFrom(seed());

    expect(bodyReplaced(draft)).toBe(false);
    expect(toRequest({ ...draft, body: '{"name":"alice"}' }).fields).toEqual({ body: '{"name":"alice"}' });
  });

  it("a cleared text body is an edit, and sends empty", () => {
    const draft = { ...draftFrom(seed()), body: "" };

    expect(bodyRevertible(draft)).toBe(true);
    expect(toRequest(draft).fields).toEqual({ body: "" });
  });

  it("revert restores the seed's text", () => {
    const draft = revertBody({ ...draftFrom(seed()), body: "x" });

    expect(draft.body).toBe('{"name":"a"}');
    expect(bodyRevertible(draft)).toBe(false);
  });

  it.each<BodyView>([
    { state: "not_captured", reason: "too_large", actual: 3_355_443 },
    { state: "captured", size: 2, data_base64: btoa(String.fromCharCode(0xff, 0x00)) },
  ])("over an absent or binary seed body, a blank field sends nothing and typing replaces it", (body) => {
    const draft = draftFrom(seed({ request_body: body }));

    expect(toRequest(draft).fields).toEqual({});
    expect(bodyRevertible(draft)).toBe(false);
    expect(toRequest({ ...draft, body: "typed" }).fields).toEqual({ body: "typed" });
  });

  it("over an empty seed body, typing sends it", () => {
    const draft = draftFrom(seed({ request_body: { state: "empty" } }));

    expect(toRequest({ ...draft, body: "x" }).fields).toEqual({ body: "x" });
    expect(toRequest(draft).fields).toEqual({});
  });
});

describe("the request a replay sends", () => {
  it("a changed method alone is the only field", () => {
    expect(toRequest({ ...draftFrom(seed()), method: "PUT" })).toEqual({ seedId: "seed-1", fields: { method: "PUT" } });
  });

  it("a removed row sends the rest of the list", () => {
    const start = draftFrom(seed());
    const draft = removeRow(start, rowNamed(start, "Cookie").key);

    expect(toRequest(draft).fields.headers).toEqual([
      { name: "Content-Type", value: "application/json" },
      { name: "Authorization", value: "Bearer OLD" },
    ]);
  });

  it("removing every row sends an empty list, not the seed's", () => {
    let draft = draftFrom(seed());
    for (const row of draft.rows) draft = removeRow(draft, row.key);

    expect(toRequest(draft).fields.headers).toEqual([]);
  });
});

describe("a draft from scratch", () => {
  it("starts empty, as GET", () => {
    expect(blankDraft()).toEqual({ seed: null, method: "GET", url: "", rows: [], body: "" });
    expect(isEdited(blankDraft())).toBe(false);
  });

  it("sends method and URL always, headers when there are rows, a body when one was typed", () => {
    const bare = { ...blankDraft(), url: "https://a.test/" };
    expect(toRequest(bare)).toEqual({ seedId: null, fields: { method: "GET", url: "https://a.test/" } });

    const { draft, key } = withRow({ ...bare, method: "POST", body: "{}" });
    expect(toRequest(updateRow(draft, key, { name: "X-A", value: "1" }))).toEqual({
      seedId: null,
      fields: { method: "POST", url: "https://a.test/", headers: [{ name: "X-A", value: "1" }], body: "{}" },
    });
  });

  it("is edited once anything is in it", () => {
    expect(isEdited({ ...blankDraft(), url: "h" })).toBe(true);
    expect(isEdited({ ...blankDraft(), method: "PUT" })).toBe(true);
    expect(isEdited({ ...blankDraft(), body: "x" })).toBe(true);
    expect(isEdited(withRow(blankDraft()).draft)).toBe(true);
  });
});

describe("send and start over", () => {
  it("send is gated on a URL and on nothing else", () => {
    expect(canSend(blankDraft())).toBe(false);
    expect(canSend({ ...blankDraft(), url: "   " })).toBe(false);
    expect(canSend({ ...blankDraft(), url: "htp:/x" })).toBe(true);
  });

  it("start over returns a seeded draft to its seed and a blank one to blank", () => {
    const start = draftFrom(seed());
    const edited = { ...removeRow(start, start.rows[1].key), url: "https://b.test/", body: "" };

    expect(restart(edited)).toEqual(start);
    expect(restart({ ...blankDraft(), url: "x" })).toEqual(blankDraft());
  });
});
