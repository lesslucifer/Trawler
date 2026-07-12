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
      },
      fontFamily: {
        ui: ['Inter', 'system-ui', 'sans-serif'],
        mono: ['"DM Mono"', 'ui-monospace', 'monospace'],
        serif: ['"Instrument Serif"', 'serif'],
      },
      spacing: {
        2: "8px",
        3: "12px",
        4: "16px",
        6: "24px",
        8: "32px",
        12: "48px",
      },
      borderRadius: {
        button: "3px",
        card: "4px",
        lg: "6px",
      },
    },
  },
  plugins: [],
};
