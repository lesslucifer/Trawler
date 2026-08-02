const METHOD_COLOR: Record<string, string> = {
  GET: "text-method-get",
  POST: "text-method-post",
  PUT: "text-method-put",
  DELETE: "text-method-delete",
  PATCH: "text-method-patch",
};

/**
 * MethodLabel — the HTTP verb of a flow; uppercase mono, fixed-width slot, color-coded per verb.
 * @tier T2 — owns the verb→color mapping (GET/POST/PUT/DELETE/PATCH → method tokens, else body tone), mirroring StatusPill's status→tone
 * @anatomy uppercase verb, mono, fixed-width (4.5ch min), colored by the mapping — one part, no interactivity
 * @use anywhere a flow's method must be shown: list rows, the identity bar — pass the raw method string, nothing else
 * @avoid per-call-site verb styling — the mapping lives here; a new color is a token + mapping change in this component, never an inline tint (DEC-11)
 * @states GET(method-get) · POST(method-post) · PUT(method-put) · DELETE(method-delete) · PATCH(method-patch) · HEAD/OPTIONS/CONNECT/TRACE/other(ink-body fallback). No interaction states — it is display text.
 * @madeof —
 */
export function MethodLabel({ method, className = "" }: { method: string; className?: string }) {
  const verb = method.toUpperCase();

  return (
    <span
      className={`inline-block min-w-[4.5ch] font-mono text-body ${METHOD_COLOR[verb] ?? "text-ink-body"} ${className}`}
    >
      {verb}
    </span>
  );
}
