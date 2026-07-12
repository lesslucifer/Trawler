import type { ReactNode } from "react";

// Base app layout — the named regions later UI epics slot into: the header/filter
// bar, the main content, and a reserved detail region (E-UIINSPECT's inspector).
export function Layout({
  header,
  children,
  detail,
}: {
  header: ReactNode;
  children: ReactNode;
  detail?: ReactNode;
}) {
  return (
    <div className="flex h-full flex-col bg-canvas text-ink">
      <header className="flex items-center gap-3 border-b border-hairline bg-canvas-soft px-4 py-2">
        {header}
      </header>
      <div className="flex min-h-0 flex-1">
        <main className="min-w-0 flex-1 overflow-hidden">{children}</main>
        {detail != null && (
          <aside className="w-[380px] shrink-0 overflow-auto border-l border-hairline bg-canvas-soft">
            {detail}
          </aside>
        )}
      </div>
    </div>
  );
}
