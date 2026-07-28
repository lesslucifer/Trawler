import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { Gallery } from "./Gallery"; // ponytail: W5 step 4 verification screen — delete with Gallery.tsx
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

const gallery = new URLSearchParams(location.search).has("gallery");

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{gallery ? <Gallery /> : <App />}</React.StrictMode>,
);
