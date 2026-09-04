import { createContext, useContext, type ReactNode } from "react";

const InBand = createContext(false);

/**
 * Section — titled grouping container; heading + spacing + hairline separation, no logic.
 * @tier T1 — layout chrome only; heading typography and hairline rhythm would drift per call site without it
 * @anatomy uppercase-muted heading (Inter xs, letter-spaced) · transparent content slot (no overflow, no layout) · hairline bottom border between siblings · variant="band": the label is the group's h2 over member Sections at h3, in a group slot that adds no padding (F3 — exactly two levels, never three)
 * @use every labelled zone of a detail or settings screen: request, response, headers, body panels · variant="band" wherever a screen needs a second grouping level (a two-leg panel's leg level, or a label for a stack of Sections)
 * @avoid unlabelled grouping (plain spacing does it) · a Section inside another Section's CONTENT slot, and any band nested in a band (two levels is the cap — use variant="band" for the second, never nesting) · a Section that needs to collapse (put a Disclosure inside the slot instead — never a Section inside a Disclosure's summary, which would hide a heading) · a screen drawing its own group label above a Section stack (that is what variant="band" is for)
 * @states none of its own — all interactive states belong to slot children
 * @madeof —
 */
export function Section({
  title,
  children,
  variant = "default",
}: {
  title: string;
  children: ReactNode;
  variant?: "default" | "band";
}) {
  const inBand = useContext(InBand);

  if (variant === "band") {
    return (
      <section data-variant="band">
        <h2 className="mb-2 px-4 pt-3 text-chrome uppercase tracking-label text-ink-strong">
          {title}
        </h2>
        <InBand.Provider value={true}>{children}</InBand.Provider>
      </section>
    );
  }

  const Heading = inBand ? "h3" : "h2";

  return (
    <section className="border-b border-hairline px-4 py-3">
      <Heading className="mb-2 text-chrome uppercase tracking-label text-ink-mute">{title}</Heading>
      {children}
    </section>
  );
}
