const CONTENT_SECURITY_POLICY = "default-src 'none'; style-src 'unsafe-inline'; img-src data:; form-action 'none'";
const PAGE_DEFAULT = ":root{color-scheme:light;background:Canvas;color:CanvasText}";
const GUARDS = `<meta http-equiv="Content-Security-Policy" content="${CONTENT_SECURITY_POLICY}"><base target="_blank"><style>${PAGE_DEFAULT}</style>`;
const REMOVED = "meta[http-equiv], base, template, link, iframe, frame, frameset, object, embed";
const MATHML = "http://www.w3.org/1998/Math/MathML";
const DOCTYPE = "<!doctype html>";
const MOST_PASSES = 5;

function cleaned(source: string): string {
  const page = new DOMParser().parseFromString(source, "text/html");
  page.querySelectorAll(REMOVED).forEach((node) => node.remove());
  page.querySelectorAll("a, area").forEach((link) => {
    link.setAttribute("target", "_blank");
    link.removeAttribute("ping");
    link.removeAttribute("download");
  });
  page.querySelectorAll("[href]").forEach((node) => {
    if (node.namespaceURI === MATHML) node.removeAttribute("href");
  });
  return `${page.doctype ? DOCTYPE : ""}${page.documentElement.outerHTML}`;
}

function settled(source: string): string | null {
  let current = source;
  for (let pass = 0; pass < MOST_PASSES; pass++) {
    const next = cleaned(current);
    if (next === current) return current;
    current = next;
  }
  return null;
}

function inert(source: string): string {
  const page = new DOMParser().parseFromString("<pre></pre>", "text/html");
  page.querySelector("pre")!.textContent = source;
  return page.documentElement.outerHTML;
}

export function previewDocument(source: string): string {
  const body = settled(source) ?? inert(source);
  return body.startsWith(DOCTYPE) ? DOCTYPE + GUARDS + body.slice(DOCTYPE.length) : GUARDS + body;
}
