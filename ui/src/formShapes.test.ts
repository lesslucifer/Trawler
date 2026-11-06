import { describe, expect, it } from "vitest";
import { addPairAfter, editPair, formShape, labelShape, parsePairs, readForm, removePair, repeats, sortByKey } from "./formPairs";

const OWNER_HEADERS = [
  "content-type: image/jpeg",
  "x-storage-error: unified_born",
  "last-modified: Mon, 01 Jan 2024 04:35:16 GMT",
  "x-needle-checksum: 3721864201",
  "x-fb-debug: 9cR0yq",
  "x-fb-trip-id: 1871",
  "server: proxygen-bolt",
  "cross-origin-resource-policy: cross-origin",
  "timing-allow-origin: *",
  "access-control-allow-origin: *",
  "cache-control: max-age=1209600, no-transform",
  "accept-ranges: bytes",
  "content-length: 67720",
  "x-fb-connection-quality: GOOD; q=0.7, rtt=127, rtx=1, c=26, mss=1380, tbw=161754, tp=-1, tpl=-1, uplat=1, ullat=-1",
  "date: Fri, 06 Nov 2026 04:14:01 GMT",
  'alt-svc: h3=":443"; ma=86400',
  'no-vary-search: key-order, params=("_nc_gid" "oh" "oe" "_nc_oc" "_nc_ohc" "_nc_cat" "_nc_ht" "_nc_cb")',
  "x-fb-ptm-uuid: A25FDB4FF9B946EB86583B1162E63BEA",
  "x-robots-tag: noarchive, noindex",
].join("\n");

const EFG =
  "eyJ2ZW5jb2RlX3RhZyI6ImRhc2hfcjJhdjEtcjFnZW4yX3ZpZGVvIiwidmlkZW9faWQiOjExNjcwNDE4NjA5Mzk5MCwiYmFzZWxpbmVfYml0cmF0ZSI6MTMzMDMzLCJ1cmxnZW5fc291cmNlIjoid3d3In0=";

const OWNER_URI =
  "https://scontent.fper7-1.fna.fbcdn.net/o1/v/t2/f2/m366/AQO6_oXZXBVO-Vg63B_dSHLaVAkzUN1KTA-quA9YU-KOJ8jFZHXMNeNcWFy9tGJGRvwJO2_xwYZjZDOKyBG6UwneCStCZRiRaPAaJ-c.mp4" +
  "?_nc_cat=106&_nc_oc=Adre6PA0SJXUbWCYQOZbdr71A2l2k9xPK-mY33qIB_kHL0SAPfYd_KcnXffbTZgEHlBj70CPxkrDPtpGSOkTuwAU&_nc_sid=9ca052" +
  "&_nc_ht=scontent.fper7-1.fna.fbcdn.net&_nc_ohc=SRjSZkZxTz0Q7kNvwFcBnNN&efg=" +
  EFG +
  "&ccb=17-1&_nc_gid=3jOMaZpmvAoWLl9Wahjz7w&_nc_ss=7c2a8&_nc_map=urlgen_bucketless&_nc_zt=28&oh=00_AQSKLtnQxZO8Rr6ksmF6FoafSaJwGVJ2fdwENZPLGt1xQg&oe=6AF30F6F&bytestart=1051";

const rows = (text: string) => {
  const { parts, pairs } = readForm(text, formShape(text));
  return { parts: parts.map((p) => [p.key, p.value]), pairs: pairs.map((p) => [p.key, p.value]) };
};

describe("the shape a form's text is read in", () => {
  it.each([
    [OWNER_HEADERS, "headers"],
    [OWNER_URI, "url"],
    ["/search?q=a+b", "url"],
    ["//cdn.example/x.js?v=1", "url"],
    ["http://[::1]:8080/", "url"],
    ["https://例え.jp/パス?キー=値", "url"],
    ["grant_type=password&scope=a+b", "query"],
    ["a=hello world", "query"],
    ["", "query"],
    ["a=1\n", "query"],
    ["Host: example.com", "headers"],
    [":status: 200\nserver: x", "headers"],
    ["a: 1\n  folded\nb: 2", "headers"],
    ['{"a":"b=c"}', "none"],
    ["<p>a=b</p>", "none"],
    ["line one\nline two", "none"],
    ["a=1\nb=2", "none"],
    ["  indented: first\nb: 2", "none"],
    ["https://a.example/ b", "query"],
  ])("%j reads as %s", (text, shape) => {
    expect(formShape(text)).toBe(shape);
  });
});

describe("headers read as name: value lines", () => {
  it("the owner's 19 response headers are 19 rows, each value whole", () => {
    const { pairs } = rows(OWNER_HEADERS);
    expect(pairs).toHaveLength(19);
    expect(pairs[8]).toEqual(["timing-allow-origin", "*"]);
    expect(pairs[10]).toEqual(["cache-control", "max-age=1209600, no-transform"]);
    expect(pairs[15]).toEqual(["alt-svc", 'h3=":443"; ma=86400']);
    expect(pairs[2]).toEqual(["last-modified", "Mon, 01 Jan 2024 04:35:16 GMT"]);
  });

  it("repeats stay rows of their own; a fold joins its line; a pseudo-header keeps its colon; values are not decoded", () => {
    const text = "set-cookie: a=1\nset-cookie: b=%20\n:status: 200\nx-long: one\n\ttwo\nempty:\nno-colon";
    const pairs = parsePairs(text, "headers");
    expect(pairs.map((p) => [p.key, p.value])).toEqual([
      ["set-cookie", "a=1"],
      ["set-cookie", "b=%20"],
      [":status", "200"],
      ["x-long", "one two"],
      ["empty", ""],
      ["no-colon", ""],
    ]);
    expect([...repeats(pairs)]).toEqual([
      [0, { nth: 1, of: 2 }],
      [1, { nth: 2, of: 2 }],
    ]);
    expect(pairs.some((p) => p.kept)).toBe(false);
  });

  it("CRLF lines read the same", () => {
    expect(parsePairs("a: 1\r\nb: 2\r\n", "headers").map((p) => [p.key, p.value])).toEqual([
      ["a", "1"],
      ["b", "2"],
    ]);
  });

  it("an edit rewrites only its own line; a value's line breaks become spaces", () => {
    const text = "a: 1\nb:  2\nc: 3";
    expect(editPair(text, 1, "value", "x\ny", "headers")).toBe("a: 1\nb: x y\nc: 3");
    expect(editPair(text, 1, "key", "B", "headers")).toBe("a: 1\nB:  2\nc: 3");
    expect(editPair("a: 1\r\nb: 2\r\n", 0, "value", "9", "headers")).toBe("a: 9\r\nb: 2\r\n");
  });

  it("add, remove and sort work on lines and keep the trailing break", () => {
    expect(addPairAfter("a: 1\nb: 2\n", 0, "headers")).toBe("a: 1\n:\nb: 2\n");
    expect(parsePairs(addPairAfter("a: 1\n", 0, "headers"), "headers")).toHaveLength(2);
    expect(addPairAfter("", -1, "headers")).toBe(":");
    expect(parsePairs(":", "headers")).toEqual([{ index: 0, raw: ":", key: "", value: "", kept: false }]);
    expect(removePair("a: 1\nb: 2\n  more\nc: 3", 1, "headers")).toBe("a: 1\nc: 3");
    expect(sortByKey("b: 2\na: 1\n", "headers")).toBe("a: 1\nb: 2\n");
  });

  it("an edited key keeps only what a field name can hold, so it reads back as typed", () => {
    expect(editPair("x: v", 0, "key", "a:b c", "headers")).toBe("abc: v");
    expect(parsePairs(editPair("x: v", 0, "key", "a:b c", "headers"), "headers")[0]).toMatchObject({ key: "abc", value: "v" });
    expect(editPair("x: v", 0, "key", ":authority", "headers")).toBe(":authority: v");
    expect(editPair("no-colon", 0, "key", "a b", "headers")).toBe("ab");
  });

  it("a row with no name keeps a value holding a colon whole", () => {
    const valued = editPair(addPairAfter("a: 1", 0, "headers"), 1, "value", "http://x", "headers");
    expect(valued).toBe("a: 1\n: http://x");
    expect(parsePairs(valued, "headers")[1]).toMatchObject({ key: "", value: "http://x" });
    expect(parsePairs(":status: 200\n:: x", "headers").map((p) => [p.key, p.value])).toEqual([
      [":status", "200"],
      ["", ": x"],
    ]);
  });

  it("a new pair's key and value land on its own line", () => {
    const added = addPairAfter("a: 1", 0, "headers");
    const keyed = editPair(added, 1, "key", "x-new", "headers");
    expect(editPair(keyed, 1, "value", "v", "headers")).toBe("a: 1\nx-new: v");
  });
});

describe("a URL reads as labelled parts, then its query's pairs", () => {
  it("the owner's URI: scheme, host and path are parts; _nc_cat is the first pair", () => {
    const { parts, pairs } = rows(OWNER_URI);
    expect(parts).toEqual([
      ["scheme", "https"],
      ["host", "scontent.fper7-1.fna.fbcdn.net"],
      [
        "path",
        "/o1/v/t2/f2/m366/AQO6_oXZXBVO-Vg63B_dSHLaVAkzUN1KTA-quA9YU-KOJ8jFZHXMNeNcWFy9tGJGRvwJO2_xwYZjZDOKyBG6UwneCStCZRiRaPAaJ-c.mp4",
      ],
    ]);
    expect(pairs).toHaveLength(14);
    expect(pairs[0]).toEqual(["_nc_cat", "106"]);
    expect(pairs[5]).toEqual(["efg", EFG]);
    expect(pairs[13]).toEqual(["bytestart", "1051"]);
  });

  it("user, port and fragment are parts; parts are percent-decoded without + as space; the query takes + as space", () => {
    const { parts, pairs } = rows("https://us%20er:pw@host.example:8443/a%2Fb+c/?q=a+b%26c&q=2#sec%20one");
    expect(parts).toEqual([
      ["scheme", "https"],
      ["user", "us er:pw"],
      ["host", "host.example"],
      ["port", "8443"],
      ["path", "/a/b+c/"],
      ["fragment", "sec one"],
    ]);
    expect(pairs).toEqual([
      ["q", "a b&c"],
      ["q", "2"],
    ]);
  });

  it("an IPv6 host keeps its brackets; a non-ASCII host and path read as written", () => {
    expect(rows("http://[::1]:8080/x").parts).toEqual([
      ["scheme", "http"],
      ["host", "[::1]"],
      ["port", "8080"],
      ["path", "/x"],
    ]);
    expect(rows("https://例え.jp/パス?キー=%E5%80%A4")).toEqual({
      parts: [
        ["scheme", "https"],
        ["host", "例え.jp"],
        ["path", "/パス"],
      ],
      pairs: [["キー", "値"]],
    });
  });

  it("a relative URL has a path part only; no query means no pairs; a bare ? means no pairs", () => {
    expect(rows("/search?q=1&flag")).toEqual({ parts: [["path", "/search"]], pairs: [["q", "1"], ["flag", ""]] });
    expect(rows("https://a.example/x#top")).toEqual({
      parts: [
        ["scheme", "https"],
        ["host", "a.example"],
        ["path", "/x"],
        ["fragment", "top"],
      ],
      pairs: [],
    });
    expect(rows("https://a.example/x?").pairs).toEqual([]);
    expect(rows("//cdn.example/x.js?v=1").parts).toEqual([
      ["host", "cdn.example"],
      ["path", "/x.js"],
    ]);
  });

  it("a bad escape never throws: the part or pair is kept as sent and says so", () => {
    const { parts, pairs } = readForm("https://a.example/%E0%A4%A?x=%ZZ&y=1#%", "url");
    expect(parts.find((p) => p.key === "path")).toMatchObject({ value: "/%E0%A4%A", kept: true });
    expect(parts.find((p) => p.key === "fragment")).toMatchObject({ value: "%", kept: true });
    expect(pairs[0]).toMatchObject({ key: "x", value: "%ZZ", kept: true });
    expect(pairs[1]).toMatchObject({ key: "y", value: "1", kept: false });
  });

  it("a URL inside a value stays one value", () => {
    expect(rows("https://a.example/r?next=https%3A%2F%2Fb.example%2F%3Fx%3D1&n=2").pairs).toEqual([
      ["next", "https://b.example/?x=1"],
      ["n", "2"],
    ]);
  });

  it("parts are numbered before the pairs, pairs from 0", () => {
    const { parts, pairs } = readForm("https://a.example/?k=v", "url");
    expect(parts.map((p) => p.index)).toEqual([-3, -2, -1]);
    expect(pairs.map((p) => p.index)).toEqual([0]);
  });

  it("edits, add, remove and sort touch only the query; head and fragment keep their bytes", () => {
    const url = "https://A.example:443/p%20q?b=2&a=1#frag";
    expect(editPair(url, 0, "value", "x y", "url")).toBe("https://A.example:443/p%20q?b=x%20y&a=1#frag");
    expect(addPairAfter(url, 1, "url")).toBe("https://A.example:443/p%20q?b=2&a=1&=#frag");
    expect(removePair(url, 0, "url")).toBe("https://A.example:443/p%20q?a=1#frag");
    expect(removePair("https://a.example/?a=1#f", 0, "url")).toBe("https://a.example/#f");
    expect(sortByKey(url, "url")).toBe("https://A.example:443/p%20q?a=1&b=2#frag");
    expect(addPairAfter("https://a.example/x#f", -1, "url")).toBe("https://a.example/x?=#f");
    expect(sortByKey("https://a.example/x?#f", "url")).toBe("https://a.example/x?#f");
    expect(editPair("https://a.example/x?", 3, "value", "v", "url")).toBe("https://a.example/x?");
  });
});

describe("a label that names a form names its shape; bytes are read only without one", () => {
  it.each([
    ["application/x-www-form-urlencoded; charset=utf-8", "query"],
    ["text/rfc822-headers", "headers"],
    ["TEXT/URI-LIST", "url"],
    ["text/plain", null],
    ["constructor", null],
    [null, null],
  ])("%s → %s", (label, shape) => {
    expect(labelShape(label)).toBe(shape);
  });

  it("parts carry a part flag, pairs do not", () => {
    const { parts, pairs } = readForm("https://a.example/?k=v", "url");
    expect(parts.every((part) => part.part === true)).toBe(true);
    expect(pairs.every((pair) => pair.part === undefined)).toBe(true);
  });
});

describe("text that is none of the shapes", () => {
  it("reads no rows", () => {
    expect(readForm('{"a":1}', "none")).toEqual({ parts: [], pairs: [] });
  });
});
