/** @type {import('tailwindcss').Config} */
// Warp DESIGN.md tokens (see docs/decisions DEC-11). Warm-dark chrome + off-white
// ink, zero chromatic accent — status colors are the deliberate exception a
// debugging proxy needs to tell 2xx/4xx/5xx apart at a glance.
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        canvas: "#2b2622",
        "canvas-soft": "#383330",
        hairline: "#3f3a36",
        ink: "#f7f5f0",
        "ink-strong": "#dad2c1",
        "ink-body": "#c9c0ad",
        "ink-mute": "#aea69c",
        ok: "#4cc38a",
        warn: "#e5484d",
        // Syntax tints for rendered payloads. Chromatic by necessity — a JSON
        // body is unreadable without them — and scoped to body content only,
        // never to chrome. Admitted to the vocabulary by E-DFLOW.W5 so a
        // specimen can draw one; whether the rulebook permits them is W6's.
        "syn-key": "#8fb8de",
        "syn-str": "#9ec99a",
        "syn-num": "#d9a866",
        // Method tints — MethodLabel fork D3. Same W5/W6 split as the syntax
        // tokens: expressible here, permissibility is W6's.
        "method-get": "#4cc38a",
        "method-post": "#e5bf4d",
        "method-put": "#e5924d",
        "method-delete": "#e5484d",
        "method-patch": "#a78bfa",
      },
      fontFamily: {
        ui: ['Inter', 'system-ui', 'sans-serif'],
        mono: ['"DM Mono"', 'ui-monospace', 'monospace'],
        serif: ['"Instrument Serif"', 'serif'],
      },
      // The layout scale (8px steps) plus the control-chrome tokens the
      // component collection needs. Chrome sits below the layout scale's first
      // step by design: a 24px control with 11px text cannot be built from 8px
      // increments. Each chrome key names a decision — never add a raw step,
      // or the scale stops constraining anything (E-DFLOW.W5 step 3, D2).
      spacing: {
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
        "grid-gap-x": "12px",
        "grid-gap-y": "4px",
        "step-marker": "18px",
        "step-gap": "10px",
        control: "24px",
        "control-lg": "28px",
        glyph: "9px",
        spinner: "10px",
        "spinner-lg": "12px",
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
      },
      borderRadius: {
        button: "3px",
        card: "4px",
        lg: "6px",
        panel: "8px",
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
      opacity: {
        disabled: ".38",
      },
    },
  },
  plugins: [],
};
