import { useEffect, useRef, type ReactNode } from "react";
import { IconButton } from "./components/index";

// summary is a native tab stop, so it belongs here: a trap whose stop list
// disagrees with the browser's tab ring wraps at the wrong element.
const FOCUSABLE =
  'a[href], button:not([disabled]), summary, input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

// Build-local by warrant, and a module of its own since the second and third
// sightings arrived: no registry entry owns modality, and a screen importing a
// frame out of a sibling screen is the shape the collection exists to prevent.
// Promoting it into the collection is design run 11's proposal and the
// collection's to perform — using it is not.
export function ModalShell({
  title,
  onCancel,
  children,
  testId,
  mode,
  dismissLabel = "cancel",
}: {
  title: string;
  onCancel: () => void;
  children: ReactNode;
  // Names all three of this dialog's parts — `<testId>-dialog`, `-backdrop`,
  // `-panel` — so a screen with two dialogs cannot address the wrong one.
  testId: string;
  mode?: string;
  // The ✕'s accessible name. A dialog whose own escape is also called "cancel"
  // would ship two controls under one name doing the same thing.
  dismissLabel?: string;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const invoker = useRef<Element | null>(null);

  useEffect(() => {
    invoker.current = document.activeElement;
    // Whatever the caller marked, not the first thing in the DOM: that is the ✕,
    // and an opening dialog must not put its escape under the first Enter — nor
    // the manual-steps summary, which answers nothing. The marker may be the
    // focusable itself (a textarea) or wrap one (a Button).
    const panelEl = panel.current;
    const marked = panelEl?.querySelector<HTMLElement>('[data-opener="true"]');
    const action = marked?.matches(FOCUSABLE)
      ? marked
      : marked?.querySelector<HTMLElement>(FOCUSABLE);
    (action ?? panelEl?.querySelector<HTMLElement>(FOCUSABLE))?.focus();
    return () => {
      (invoker.current as HTMLElement | null)?.focus?.();
    };
  }, []);

  const trap = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") {
      // A nested control that already consumed it — Select closing its popup —
      // marks the event handled. Dismissing a dropdown is not dismissing the
      // dialog.
      if (!event.defaultPrevented) onCancel();
      return;
    }
    if (event.key !== "Tab" || !panel.current) return;
    const stops = [...panel.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
    if (stops.length === 0) return;
    const edge = event.shiftKey ? stops[0] : stops[stops.length - 1];
    if (document.activeElement === edge) {
      event.preventDefault();
      (event.shiftKey ? stops[stops.length - 1] : stops[0]).focus();
    }
  };

  return (
    <div
      data-testid={`${testId}-dialog`}
      data-mode={mode}
      role="dialog"
      aria-modal="true"
      aria-label={title}
      onKeyDown={trap}
      className="fixed inset-0 z-modal flex items-center justify-center bg-scrim"
    >
      <button
        type="button"
        aria-hidden="true"
        tabIndex={-1}
        data-testid={`${testId}-backdrop`}
        onClick={onCancel}
        className="absolute inset-0 h-full w-full cursor-default"
      />
      <div
        ref={panel}
        data-testid={`${testId}-panel`}
        className="relative w-full max-w-dialog-max rounded-lg border border-hairline bg-canvas p-4 text-ink"
      >
        <div className="mb-3 flex items-start justify-between gap-2">
          <h2 className="font-serif text-title italic">{title}</h2>
          <IconButton glyph="close" label={dismissLabel} onClick={onCancel} />
        </div>
        {children}
      </div>
    </div>
  );
}
