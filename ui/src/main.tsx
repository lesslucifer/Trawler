import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/dm-mono/400.css";
import "@fontsource/instrument-serif/400-italic.css";
import "./index.css";

// e2e-only: load the wdio frontend plugin (execute API). Static string import so
// release builds tree-shake it out entirely; the dep lives in devDependencies.
if (import.meta.env.VITE_E2E) {
  void import("@wdio/tauri-plugin");
}

const root = ReactDOM.createRoot(document.getElementById("root")!);
const mount = (screen: React.ReactNode) =>
  root.render(<React.StrictMode>{screen}</React.StrictMode>);

// ponytail: W5 step 4 verification screen — delete with Gallery.tsx. Lazily
// imported under a dev-only guard so release builds tree-shake it out, the way
// the wdio plugin above is: a static import pulls the whole collection into the
// shipped bundle for a screen no user can reach.
if (import.meta.env.DEV && new URLSearchParams(location.search).has("gallery")) {
  void import("./Gallery").then(({ Gallery }) => mount(<Gallery />));
} else {
  mount(<App />);
}
