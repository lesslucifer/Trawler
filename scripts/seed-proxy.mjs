#!/usr/bin/env node
// Seed the Trawler proxy with one flow per inspector case, so the native UI can
// be checked by hand. Stands up a local origin covering every body type,
// encoding, charset, status, method, and body state, then fires proxied requests
// at it through curl (matching the `curl -x` quick-check idiom).
//
//   node scripts/seed-proxy.mjs                 # proxy at 127.0.0.1:7420
//   PROXY=http://127.0.0.1:7420 node scripts/seed-proxy.mjs
//
// After seeding, the origin stays up so you can fire your own proxied requests
// against it (Ctrl-C to stop). Set ORIGIN_PORT for a stable URL across runs.

import http from "node:http";
import zlib from "node:zlib";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const PROXY = process.env.PROXY ?? "http://127.0.0.1:7420";
const ORIGIN_PORT = Number(process.env.ORIGIN_PORT ?? 0);
const run = promisify(execFile);

const PNG_1x1 = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  "base64",
);
const JSON_BODY = JSON.stringify({ hello: "world", nested: { a: [1, 2, 3] }, n: 42 }, null, 0);
const XML_ONE_LINE =
  '<?xml version="1.0"?><catalog><book id="1"><title>A</title></book><book id="2"><title>B</title></book></catalog>';

function send(res, status, headers, body) {
  res.writeHead(status, headers);
  res.end(body);
}

const origin = http.createServer((req, res) => {
  const path = req.url.split("?")[0];
  switch (`${path}`) {
    case "/json":
      return send(res, 200, { "content-type": "application/json" }, JSON_BODY);
    case "/xml":
      return send(res, 200, { "content-type": "application/xml" }, XML_ONE_LINE);
    case "/html":
      return send(res, 200, { "content-type": "text/html" }, "<html><body><h1>Hi</h1><p>para</p></body></html>");
    case "/text":
      return send(res, 200, { "content-type": "text/plain" }, "just some text\nwith two lines");
    case "/image":
      return send(res, 200, { "content-type": "image/png" }, PNG_1x1);
    case "/binary":
      return send(res, 200, { "content-type": "application/octet-stream" }, Buffer.from([0, 1, 2, 250, 251, 255, 13, 10, 127]));
    case "/gzip":
      return send(res, 200, { "content-type": "application/json", "content-encoding": "gzip" }, zlib.gzipSync(JSON_BODY));
    case "/brotli":
      return send(res, 200, { "content-type": "application/json", "content-encoding": "br" }, zlib.brotliCompressSync(JSON_BODY));
    case "/shiftjis":
      return send(res, 200, { "content-type": "text/plain; charset=Shift_JIS" }, iconvShiftJis("文字化けテスト"));
    case "/iso":
      return send(res, 200, { "content-type": "text/plain; charset=ISO-8859-1" }, Buffer.from("café £5 — naïve", "latin1"));
    case "/empty":
      return send(res, 204, {}, null);
    case "/notfound":
      return send(res, 404, { "content-type": "text/plain" }, "no such thing");
    case "/error":
      return send(res, 500, { "content-type": "application/json" }, '{"error":"boom","code":500}');
    case "/slow":
      return setTimeout(() => send(res, 200, { "content-type": "application/json" }, '{"slow":true}'), 800);
    case "/large":
      return send(res, 200, { "content-type": "application/octet-stream", "content-length": String(11 * 1024 * 1024) }, Buffer.alloc(11 * 1024 * 1024, 0x41));
    case "/echo":
    case "/form":
    case "/resource": {
      const chunks = [];
      req.on("data", (c) => chunks.push(c));
      req.on("end", () => send(res, req.method === "DELETE" ? 204 : 200, { "content-type": "application/json" }, JSON.stringify({ method: req.method, received: Buffer.concat(chunks).length })));
      return;
    }
    default:
      return send(res, 404, { "content-type": "text/plain" }, "unknown route");
  }
});

// Shift_JIS via a tiny lookup for the demo string only — no dependency; the point
// is to hand the proxy genuinely non-UTF-8 bytes with a charset label.
function iconvShiftJis(_s) {
  return Buffer.from([0x95, 0xb6, 0x8e, 0x9a, 0x89, 0xbb, 0x82, 0xaf, 0x83, 0x65, 0x83, 0x58, 0x83, 0x67]);
}

async function curl(label, method, path, extra = []) {
  const args = ["-sS", "-x", PROXY, "-o", "/dev/null", "-w", "%{http_code}", "-X", method, ...extra, `${base}${path}`];
  try {
    const { stdout } = await run("curl", args);
    console.log(`  ${String(stdout).padStart(3)}  ${method.padEnd(6)} ${path}  — ${label}`);
  } catch (e) {
    console.log(`  ERR  ${method.padEnd(6)} ${path}  — ${label}: ${e.message.split("\n")[0]}`);
  }
}

let base;
origin.listen(ORIGIN_PORT, "127.0.0.1", async () => {
  base = `http://127.0.0.1:${origin.address().port}`;
  console.log(`origin ${base}  →  proxy ${PROXY}\n`);

  await curl("JSON body → tree", "GET", "/json");
  await curl("POST with JSON body + headers → copy-as-curl --data-raw", "POST", "/echo", ["-H", "Content-Type: application/json", "-H", "X-Api-Key: secret123", "-d", '{"name":"trawler","tags":["a","b"]}']);
  await curl("XML one-liner → pretty (decode)", "GET", "/xml");
  await curl("HTML body", "GET", "/html");
  await curl("plain text", "GET", "/text");
  await curl("PNG image", "GET", "/image");
  await curl("binary → hex", "GET", "/binary");
  await curl("gzip JSON → decode", "GET", "/gzip");
  await curl("brotli JSON → decode (W1)", "GET", "/brotli");
  await curl("Shift_JIS text → charset decode (W1)", "GET", "/shiftjis");
  await curl("ISO-8859-1 text → charset decode (W1)", "GET", "/iso");
  await curl("204 empty body state", "GET", "/empty");
  await curl("404 status color", "GET", "/notfound");
  await curl("500 status color", "GET", "/error");
  await curl("~800ms → timing", "GET", "/slow");
  await curl("11MB → too_large body state", "GET", "/large");
  await curl("form-urlencoded body", "POST", "/form", ["-H", "Content-Type: application/x-www-form-urlencoded", "-d", "user=bob&pass=secret&remember=on"]);
  await curl("PUT with text body", "PUT", "/resource", ["-d", "update payload here"]);
  await curl("DELETE → 204", "DELETE", "/resource");

  console.log(`\ndone — check the rows in the UI (${PROXY.replace("7420", "7421")}/flows).`);
  console.log(`origin still up at ${base} — send your own, e.g.:`);
  console.log(`  curl -x ${PROXY} ${base}/json`);
  console.log(`Ctrl-C to stop.`);
});

process.on("SIGINT", () => origin.close(() => process.exit(0)));
