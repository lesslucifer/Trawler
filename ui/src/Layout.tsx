import type { ReactNode, RefObject } from "react";

// The app shell's four zones and, more importantly, which of them scrolls.
// The header is pinned and grows to a second line rather than clipping; the band
// is pinned under it at content height and spans the full width, because what it
// states is the app's connection and that qualifies the list and the pane alike;
// main is the single vertical scroll region, capped by the capture ring rather
// than by this file; the detail aside scrolls itself at a fixed width,
// independent of main, and is absent until a row is selected. Both app phases
// use this zoning.
export function Layout({
  header,
  band,
  children,
  detail,
  mainRef,
}: {
  header: ReactNode;
  band?: ReactNode;
  children: ReactNode;
  detail?: ReactNode;
  mainRef?: RefObject<HTMLElement>;
}) {
  return (
    <div className="flex h-full flex-col bg-canvas text-ink">
      <header className="flex flex-wrap items-center gap-3 border-b border-hairline bg-canvas-soft px-4 py-2">
        {header}
      </header>
      {band != null && (
        <div data-testid="band" className="border-b border-hairline px-4">
          {band}
        </div>
      )}
      <div className="flex min-h-0 flex-1">
        {/* tabIndex -1 keeps main out of the tab order but able to receive focus
            programmatically: the band's retry removes its own button, and focus
            has to land somewhere that exists in every state the retry resolves
            into — rows or no rows. */}
        <main ref={mainRef} tabIndex={-1} className="min-w-0 flex-1 overflow-hidden focus:outline-none">
          {children}
        </main>
        {detail != null && (
          <aside className="w-pane shrink-0 overflow-auto border-l border-hairline bg-canvas-soft">
            {detail}
          </aside>
        )}
      </div>
    </div>
  );
}
