// What the README and guide shots show: reserved hosts only (RFC 2606), no real
// token, imported so the shots never touch the public network.
type Row = {
  method: string;
  url: string;
  status: number;
  type: string;
  body: string;
  ms: number;
  version?: string;
  requestHeaders?: Record<string, string>;
};

const ORDER = JSON.stringify({
  id: "9f3c",
  status: "shipped",
  placed: "2026-11-02T09:14:00Z",
  customer: { id: 4821, name: "Ada Example", email: "ada@example.com" },
  items: [
    { sku: "A-1", title: "Field notebook", qty: 2, price: 12.5 },
    { sku: "B-7", title: "Pencil set", qty: 1, price: 6.0 },
  ],
  total: 31.0,
  currency: "EUR",
});

const PNG = "image/png";
const JSON_TYPE = "application/json";

const ROWS: Row[] = [
  { method: "GET", url: "https://www.example.org/", status: 200, type: "text/html", body: "<!doctype html><title>Example</title><h1>Shop</h1>", ms: 84 },
  { method: "GET", url: "https://cdn.example.com/assets/app.3f2a.js", status: 200, type: "text/javascript", body: "console.log('shop')", ms: 41 },
  { method: "GET", url: "https://cdn.example.com/assets/app.3f2a.css", status: 200, type: "text/css", body: "body{margin:0}", ms: 38 },
  { method: "GET", url: "https://cdn.example.com/img/logo.png", status: 200, type: PNG, body: "", ms: 22 },
  { method: "OPTIONS", url: "https://api.example.com/v1/session", status: 204, type: "text/plain", body: "", ms: 12 },
  {
    method: "POST",
    url: "https://api.example.com/v1/session",
    status: 200,
    type: JSON_TYPE,
    body: JSON.stringify({ user: 4821, expires: "2026-11-06T18:00:00Z" }),
    ms: 133,
    requestHeaders: { "content-type": "application/json", origin: "https://www.example.org", "x-request-id": "c0ffee-0042" },
  },
  { method: "GET", url: "https://api.example.com/v1/me", status: 200, type: JSON_TYPE, body: JSON.stringify({ id: 4821, name: "Ada Example" }), ms: 57 },
  { method: "OPTIONS", url: "https://api.example.com/v1/orders", status: 204, type: "text/plain", body: "", ms: 11 },
  { method: "GET", url: "https://api.example.com/v1/orders?page=1", status: 200, type: JSON_TYPE, body: JSON.stringify({ orders: ["9f3c", "9f3b", "9f3a"], next: 2 }), ms: 96 },
  { method: "GET", url: "https://api.example.com/v1/orders/9f3c", status: 200, type: JSON_TYPE, body: ORDER, ms: 71 },
  { method: "GET", url: "https://cdn.example.com/img/products/a-1.png", status: 200, type: PNG, body: "", ms: 64 },
  { method: "GET", url: "https://cdn.example.com/img/products/b-7.png", status: 200, type: PNG, body: "", ms: 59 },
  { method: "GET", url: "https://api.example.com/v1/orders/9f3c/invoice.xml", status: 200, type: "application/xml", body: '<invoice order="9f3c"><total currency="EUR">31.00</total></invoice>', ms: 88 },
  { method: "GET", url: "https://api.example.com/v1/recommendations", status: 404, type: JSON_TYPE, body: JSON.stringify({ error: "not_found" }), ms: 33 },
  { method: "POST", url: "https://api.example.com/v1/cart/items", status: 201, type: JSON_TYPE, body: JSON.stringify({ sku: "C-3", qty: 1 }), ms: 112 },
  { method: "PATCH", url: "https://api.example.com/v1/cart/items/C-3", status: 200, type: JSON_TYPE, body: JSON.stringify({ sku: "C-3", qty: 2 }), ms: 90 },
  { method: "GET", url: "https://api.example.com/v1/admin/stats", status: 401, type: JSON_TYPE, body: JSON.stringify({ error: "unauthorized" }), ms: 19 },
  { method: "POST", url: "https://telemetry.example.net/v2/events", status: 202, type: "text/plain", body: "", ms: 27 },
  { method: "GET", url: "https://api.example.com/v1/shipping/quote?to=DE", status: 503, type: JSON_TYPE, body: JSON.stringify({ error: "try_again" }), ms: 3021 },
  { method: "GET", url: "https://www.example.org/account", status: 200, type: "text/html", body: "<!doctype html><h1>Account</h1>", ms: 77 },
  { method: "POST", url: "https://auth.example.net/oauth/token", status: 200, type: JSON_TYPE, body: JSON.stringify({ token_type: "Bearer", expires_in: 3600 }), ms: 145, version: "HTTP/1.1" },
  { method: "GET", url: "https://cdn.example.com/fonts/inter.woff2", status: 200, type: "font/woff2", body: "", ms: 31 },
  { method: "DELETE", url: "https://api.example.com/v1/cart/items/B-7", status: 204, type: "text/plain", body: "", ms: 44 },
  { method: "GET", url: "https://api.example.com/v1/search?q=notebook", status: 400, type: JSON_TYPE, body: JSON.stringify({ error: "q too short" }), ms: 25 },
];

const START = Date.parse("2026-11-06T09:30:00Z");

const headers = (map: Record<string, string>) => Object.entries(map).map(([name, value]) => ({ name, value }));

export const FIXTURE = {
  log: {
    version: "1.2",
    creator: { name: "trawler-screenshots", version: "1" },
    entries: ROWS.map((row, i) => ({
      startedDateTime: new Date(START + i * 850).toISOString(),
      time: row.ms,
      request: {
        method: row.method,
        url: row.url,
        httpVersion: row.version ?? "HTTP/2.0",
        headers: headers({
          accept: row.type === JSON_TYPE ? JSON_TYPE : "*/*",
          "user-agent": "Mozilla/5.0 (Macintosh) Example/1.0",
          ...row.requestHeaders,
        }),
        queryString: [],
        cookies: [],
        headersSize: -1,
        bodySize: 0,
      },
      response: {
        status: row.status,
        statusText: "",
        httpVersion: row.version ?? "HTTP/2.0",
        headers: headers({ "content-type": row.type, "cache-control": "no-store", server: "example" }),
        cookies: [],
        content: { size: row.body.length, mimeType: row.type, text: row.body },
        redirectURL: "",
        headersSize: -1,
        bodySize: row.body.length,
      },
      cache: {},
      timings: { send: 0, wait: row.ms, receive: 0 },
    })),
  },
};

export const FIXTURE_ROWS = ROWS.length;
