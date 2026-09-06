import type { ReactNode } from "react";

// The app shell's three zones and, more importantly, which of them scrolls.
// The header is pinned and grows to a second line rather than clipping; main is
// the single vertical scroll region, capped by the capture ring rather than by
// this file; the detail aside scrolls itself at a fixed width, independent of
// main, and is absent until a row is selected. Both app phases use this zoning.
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
      <header className="flex flex-wrap items-center gap-3 border-b border-hairline bg-canvas-soft px-4 py-2">
        {header}
      </header>
      <div className="flex min-h-0 flex-1">
        <main className="min-w-0 flex-1 overflow-hidden">{children}</main>
        {detail != null && (
          <aside className="w-pane shrink-0 overflow-auto border-l border-hairline bg-canvas-soft">
            {detail}
          </aside>
        )}
      </div>
    </div>
  );
}
