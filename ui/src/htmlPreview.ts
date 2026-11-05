const CONTENT_SECURITY_POLICY = "default-src 'none'; style-src 'unsafe-inline'; img-src data:; form-action 'none'";
const PAGE_DEFAULT = ":root{color-scheme:light;background:Canvas;color:CanvasText}";

export function previewDocument(source: string): string {
  const page = new DOMParser().parseFromString(source, "text/html");
  page.querySelectorAll("meta[http-equiv], base").forEach((node) => node.remove());
  page.querySelectorAll("a, area").forEach((link) => link.setAttribute("target", "_blank"));
  const policy = page.createElement("meta");
  policy.setAttribute("http-equiv", "Content-Security-Policy");
  policy.setAttribute("content", CONTENT_SECURITY_POLICY);
  const base = page.createElement("style");
  base.textContent = PAGE_DEFAULT;
  page.head.prepend(policy, base);
  return `${page.doctype ? "<!doctype html>" : ""}${page.documentElement.outerHTML}`;
}
