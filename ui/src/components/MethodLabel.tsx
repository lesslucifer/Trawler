const METHOD_TONE: Record<string, string> = {
  GET: "text-ok",
  POST: "text-create",
  PUT: "text-replace",
  PATCH: "text-amend",
  DELETE: "text-warn",
};

/**
 * MethodLabel — the HTTP verb of a flow; uppercase mono, fixed-width slot, toned by what the request does to the resource.
 * @tier T2 — owns the verb→tone mapping (read/create/replace/amend/destroy → palette tokens, else body ink), mirroring StatusPill's status→tone
 * @anatomy uppercase verb, mono, fixed-width (min-w-verb), toned by the mapping — one part, no interactivity
 * @use anywhere a flow's method must be shown: list rows, the identity bar — pass the raw method string, nothing else
 * @avoid per-call-site verb styling — the mapping lives here; every tone is a closed-palette token, never an inline tint (DEC-17)
 * @states GET(ok) · POST(create) · PUT(replace) · PATCH(amend) · DELETE(warn) · HEAD/OPTIONS/CONNECT/TRACE/other(ink-body fallback). No interaction states — it is display text.
 * @madeof —
 */
export function MethodLabel({ method, className = "" }: { method: string; className?: string }) {
  const verb = method.toUpperCase();

  return (
    <span
      className={`inline-block min-w-verb font-mono text-body ${METHOD_TONE[verb] ?? "text-ink-body"} ${className}`}
    >
      {verb}
    </span>
  );
}
