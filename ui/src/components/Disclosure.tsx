import type { ReactNode } from "react";

/**
 * Disclosure — a summary line that reveals a slot; the app's one collapse primitive.
 * @tier T1 — summary chrome, the twist, the hit target and the slot. No business logic, and it never inspects what it hides
 * @anatomy native <details>/<summary> (fork D1 — focus, Enter/Space, aria-expanded and find-in-page come free) · the UA marker suppressed (list-none + ::-webkit-details-marker) and replaced by a › in a fixed w-glyph slot that rotates 90° when open · full-width summary row at min-h-control, ink-mute lifting to ink, wash-hover across the row · the slot adds mt-chrome-y and nothing else — no padding, border, indent or height cap
 * @use any secondary content a screen offers but does not lead with: manual setup steps, a "remove it later" remedy, an advanced block. Pass the label and the content; pass defaultOpen when the host has a reason to start it open
 * @avoid a labelled zone whose content is always visible (that is Section — and a Section that needs to collapse puts one of these inside its slot, never the reverse) · a mutually exclusive view switch (that is SegmentedControl) · a "collapsed or not at all" conditional — Disclosure always discloses; the host branches (fork D4) · a controlled open/onToggle pair: the native element owns the state and no tenant needs to override it (DEC-21) · animating the content height, capping it, or scrolling it — only the twist transitions, and unbounded content is the host zone's to cap · a caller-supplied glyph or variant
 * @states closed (ink-mute, twist 0°) · open (ink, twist 90°, slot in flow — it reflows, never overlays) · hover (wash-hover across the whole row) · focus (the shared 1px ink-mute outline; focus never moves on toggle). No disabled, empty or loading state — an empty slot is a caller bug, and a host awaiting content renders a Notice in the slot
 * @madeof — (the leaf: native <details>/<summary> is the element, the chrome is this component's)
 */
export function Disclosure({
  summary,
  children,
  defaultOpen = false,
}: {
  summary: string;
  children: ReactNode;
  defaultOpen?: boolean;
}) {
  return (
    <details
      open={defaultOpen}
      className="[&[open]>summary>[data-twist]]:rotate-90 [&[open]>summary]:text-ink"
    >
      <summary className="flex min-h-control cursor-pointer select-none list-none items-center gap-chrome-gap rounded-button font-ui text-body text-ink-mute transition-colors hover:bg-wash-hover hover:text-ink focus-visible:outline focus-visible:outline-focus focus-visible:outline-ink-mute focus-visible:outline-offset-focus [&::-webkit-details-marker]:hidden">
        <span
          aria-hidden="true"
          data-twist=""
          className="inline-block rotate-0 w-glyph text-glyph leading-none transition-transform"
        >
          ›
        </span>
        {summary}
      </summary>
      <div className="mt-chrome-y">{children}</div>
    </details>
  );
}
