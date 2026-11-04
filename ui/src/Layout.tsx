import { useEffect, useState, type ReactNode, type RefObject } from "react";
import { Splitter } from "./components/index";
import { clamp, paneBounds } from "./layoutBounds";
import type { LayoutControls } from "./useLayout";

type PaneProps = { detail: ReactNode; split: LayoutControls } | { detail?: undefined; split?: undefined };

function useInnerWidth(): number {
  const [width, setWidth] = useState(window.innerWidth);
  useEffect(() => {
    const follow = () => setWidth(window.innerWidth);
    window.addEventListener("resize", follow);
    return () => window.removeEventListener("resize", follow);
  }, []);
  return width;
}

// The app shell's four zones and, more importantly, which of them scrolls.
// The header is pinned and grows to a second line rather than clipping; the band
// is pinned under it at content height and spans the full width, because what it
// states is the app's connection and that qualifies the list and the pane alike;
// main is the single vertical scroll region, capped by the capture ring rather
// than by this file; the detail aside scrolls itself at a fixed width,
// independent of main, at the user's width clamped to what the window leaves
// the list, and is absent until a row is selected. Both app phases use this
// zoning.
export function Layout({
  header,
  band,
  children,
  detail,
  split,
  mainRef,
}: {
  header: ReactNode;
  band?: ReactNode;
  children: ReactNode;
  mainRef?: RefObject<HTMLElement>;
} & PaneProps) {
  const windowWidth = useInnerWidth();
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
        {split && detail != null && <DetailPane split={split} windowWidth={windowWidth}>{detail}</DetailPane>}
      </div>
    </div>
  );
}

function DetailPane({
  split,
  windowWidth,
  children,
}: {
  split: LayoutControls;
  windowWidth: number;
  children: ReactNode;
}) {
  const bounds = paneBounds(windowWidth, split.layout.columns);
  const width = clamp(split.layout.pane, bounds);
  return (
    <>
      <Splitter
        axis="x"
        dir={-1}
        value={width}
        min={bounds.min}
        max={bounds.max}
        onChange={split.setPane}
        onCommit={split.commit}
        onReset={split.resetPane}
        label="detail pane width"
        valueText={`${width} px`}
      />
      <aside data-testid="detail-pane" style={{ width }} className="shrink-0 overflow-auto bg-canvas-soft">
        {children}
      </aside>
    </>
  );
}
