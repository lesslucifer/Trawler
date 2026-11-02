/** @type {import('tailwindcss').Config} */
// Warp DESIGN.md tokens (see docs/decisions DEC-11). Warm-dark chrome + off-white
// ink, zero chromatic accent — status colors are the deliberate exception a
// debugging proxy needs to tell 2xx/4xx/5xx apart at a glance.
//
// The vocabulary is CLOSED (E-DFLOW.W6, DEC-17): every scale below replaces
// Tailwind's default instead of extending it, so an off-list value resolves to
// nothing. Tailwind drops an unknown utility silently, so this file is the
// definition and `npm run lint` (design/vocabulary) is the alarm.
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    colors: {
      transparent: "transparent",
      current: "currentColor",
      inherit: "inherit",
      canvas: "#2b2622",
      "canvas-soft": "#383330",
      hairline: "#3f3a36",
      ink: "#f7f5f0",
      "ink-strong": "#dad2c1",
      "ink-body": "#c9c0ad",
      "ink-mute": "#aea69c",
      // The chromatic set, named for what a colour means and never for who
      // uses it. Each carries one fixed meaning and one owning component
      // (DEC-11 chromatic-meanings); `ok` and `warn` also read as the effect a
      // request has on a resource, which is why GET and DELETE need no token
      // of their own.
      ok: "#4cc38a",
      create: "#e5bf4d",
      replace: "#e5924d",
      amend: "#a78bfa",
      warn: "#e5484d",
      // The body-pane syntax group (DEC-11, confirmed 2026-07-23). Deleted by
      // W6 step ① as unreferenced — which was wrong: SourceSurface painted all
      // three as hex literals, where no class-reading check could see them.
      "syn-key": "#8fb8de",
      "syn-str": "#9ec99a",
      "syn-num": "#d9a866",
      scrim: "rgba(0,0,0,.5)",
    },
    fontFamily: {
      ui: ["Inter", "system-ui", "sans-serif"],
      mono: ['"DM Mono"', "ui-monospace", "monospace"],
      serif: ['"Instrument Serif"', "serif"],
    },
    // The layout scale (8px steps) plus the control-chrome tokens the
    // component collection needs. Chrome sits below the layout scale's first
    // step by design: a 24px control with 11px text cannot be built from 8px
    // increments. Each chrome key names a decision — never add a raw step,
    // or the scale stops constraining anything (E-DFLOW.W5 step 3, D2).
    spacing: {
      0: "0px",
      px: "1px",
      2: "8px",
      3: "12px",
      4: "16px",
      6: "24px",
      8: "32px",
      12: "48px",
      "control-x": "9px",
      "control-y": "3px",
      "control-gap": "5px",
      "chrome-x": "8px",
      "chrome-y": "4px",
      "chrome-gap": "6px",
      "chrome-tight": "2px",
      "grid-gap-x": "12px",
      "grid-gap-y": "4px",
      "step-marker": "18px",
      "step-gap": "10px",
      control: "24px",
      "control-lg": "28px",
      "control-min": "132px",
      glyph: "9px",
      spinner: "10px",
      "spinner-lg": "12px",
      "field-sm": "80px",
      field: "128px",
      "field-lg": "160px",
      // The one-choice trigger, sized to the longest label the app offers a
      // reader ("Another Ubuntu machine") so the control never resizes with the
      // selection. A picker that changes width on every pick moves the row
      // beside it, which is the jitter Select's own min-width was reaching for.
      picker: "208px",
      "pane-narrow": "208px",
      "pane-label": "256px",
      pane: "520px",
      "listing-max": "184px",
      "body-max": "256px",
      "dialog-max": "384px",
      // Two slots sized by their content rather than by the layout scale: a
      // verb never pushes the column beside it, a one-line chrome row never
      // collapses below its glyph.
      verb: "4.5ch",
      line: "18px",
      // A collapsed wire value shows three lines (text-body at leading-normal)
      // plus the room its focus ring needs inside the clipping cell.
      "value-clamp": "60px",
      // A code a phone's camera opens from a laptop screen at arm's length: at
      // this size a certificate URL's ~33 modules each get about 4px.
      "scan-code": "128px",
      // A phone's portrait recording, held to about the height of a desktop
      // one at pane width, so a step never grows past one screen of its own.
      "step-media": "480px",
    },
    // Heights a zone may take of the panel that hosts it. Everything else
    // derives from spacing; `zone-cap` is the one relationship the panel needs:
    // a secondary zone (headers) never takes more than this share, so the
    // primary one (body) keeps the rest whatever the window height.
    maxHeight: ({ theme }) => ({
      ...theme("spacing"),
      full: "100%",
      "zone-cap": "35%",
    }),
    // Closed at one member because the UI renders one weight. Left open,
    // Tailwind's nine magnitudes all resolve and no check can say so — an open
    // scale is invisible to the lint rule by construction (E-DFLOW.W6 step 2).
    fontWeight: {
      normal: "400",
    },
    fontSize: {
      // Chrome type. The panel is dense by product requirement — a request,
      // its headers and its body on one screen — so these sit below
      // Tailwind's text-xs floor.
      label: "10px",
      chrome: "11px",
      control: "11.5px",
      body: "12px",
      value: "12.5px",
      glyph: "9px",
      "glyph-lg": "13px",
      note: "14px",
      title: "18px",
    },
    lineHeight: {
      // Only two magnitudes are ever picked; `none` is a reset, not a value.
      none: "1",
      normal: "1.5",
      loose: "1.625",
    },
    letterSpacing: {
      label: "0.025em",
    },
    borderRadius: {
      none: "0px",
      full: "9999px",
      button: "3px",
      card: "4px",
      lg: "6px",
      panel: "8px",
    },
    // `hairline` is taken by the colour scale — a width key of that name would
    // make `border-hairline` set width AND colour at once.
    borderWidth: {
      DEFAULT: "1px",
      0: "0px",
      emphasis: "1.5px",
    },
    // Named layers, not raw stacking numbers: three files each picked their
    // own before W6 and nothing related them (E-DFLOW.W6 step 0).
    zIndex: {
      sticky: "10",
      popup: "20",
      modal: "50",
    },
    outlineWidth: {
      focus: "1px",
    },
    outlineOffset: {
      focus: "2px",
      "focus-inset": "-1px",
    },
    textUnderlineOffset: {
      link: "3px",
    },
    opacity: {
      disabled: ".38",
      // Modifier steps for `bg-canvas-soft/wash` and friends — the only way a
      // colour token is dimmed without minting a second colour.
      wash: ".3",
      hairline: ".4",
      scrim: ".5",
    },
    // Hover / press washes. Ink-toned, deliberately not colours: they are the
    // zero-chromatic-accent palette's only pressed/hovered feedback.
    backgroundColor: ({ theme }) => ({
      ...theme("colors"),
      "wash-hover": "rgba(247,245,240,.07)",
      "wash-active": "rgba(247,245,240,.13)",
      "wash-tint": "rgba(174,166,156,.12)",
      "wash-ghost": "rgba(174,166,156,.06)",
      "wash-select": "rgba(174,166,156,.35)",
    }),
    boxShadow: {
      // The collection's only floating surface (Select's popup). Flagged in
      // select.html's token table as a new visual primitive — W6 admits or bans it.
      pop: "0 6px 18px rgba(0,0,0,.38)",
    },
    // maxWidth is the one derived scale that does NOT come from spacing alone —
    // Tailwind's own xs…7xl / prose / screen-* are magnitudes that survive
    // closing spacing. Redeclared to spacing plus relationship keywords only.
    maxWidth: ({ theme }) => ({
      ...theme("spacing"),
      none: "none",
      full: "100%",
      min: "min-content",
      max: "max-content",
      fit: "fit-content",
      // A reading column that answers to the window instead of to a dialog.
      // The share is what makes it grow; the ceiling is where a line of prose
      // stops being comfortable to read and is set so the longest trust command
      // still renders on one line inside a task's indented body.
      reading: "min(60%, 880px)",
    }),
    // Below: the scales that carry no magnitude the rulebook governs, declared
    // anyway because an undeclared scale inherits Tailwind's whole ladder and
    // the lint rule cannot see into it (E-DFLOW.W6 step 3 sweep). Each holds
    // exactly the members `ui/src/` draws from; the remaining 54 scales stay
    // undeclared, and any class drawn from them reds.
    animation: {
      spin: "spin 1s linear infinite",
    },
    keyframes: {
      spin: { to: { transform: "rotate(360deg)" } },
    },
    cursor: {
      default: "default",
      pointer: "pointer",
      text: "text",
    },
    flex: {
      1: "1 1 0%",
    },
    flexShrink: {
      0: "0",
    },
    gridColumnStart: {
      1: "1",
    },
    gridRowStart: {
      1: "1",
    },
    listStyleType: {
      none: "none",
      decimal: "decimal",
    },
    // Disclosure's twist is the only transform in the app: one glyph at two
    // angles, so the rotation itself carries the open/closed state.
    rotate: {
      0: "0deg",
      90: "90deg",
    },
    transitionProperty: {
      colors: "color, background-color, border-color, text-decoration-color, fill, stroke",
      transform: "transform",
    },
    extend: {},
  },
  plugins: [],
};
