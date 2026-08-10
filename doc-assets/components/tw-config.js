/* Trawler — the app's Tailwind theme, for the design docs (E-DFLOW)
 *
 * Reads ui/tailwind.config.js DIRECTLY. There is no generated copy and no
 * regeneration step: the config is an ES module, so a module script imports it
 * over file:// like any other. A specimen and its component are therefore
 * written in the same class names because they are reading the same file.
 *
 *   design file  <script type="module" src="tw-config.js"></script>
 *   design run   <script type="module" src="components/tw-config.js"></script>
 *
 * Load it after the Play CDN tag; the CDN re-scans on DOM mutation, so applying
 * the theme from an async module is safe (verified: 300/300 utilities resolve).
 *
 * It also publishes the --a-* CSS variables the doc scaffolding writes inline
 * (captions, fork cards, spec grids). Same values as the theme, derived here so
 * no doc hand-declares them -- fourteen files each carried a copy before W5.
 */
import cfg from '../../../ui/tailwind.config.js';

tailwind.config = { theme: cfg.theme };

const { colors: c, fontFamily: f } = cfg.theme;
const style = document.createElement('style');
style.textContent = `.app{
  --a-canvas:${c.canvas};--a-soft:${c['canvas-soft']};--a-line:${c.hairline};
  --a-ink:${c.ink};--a-strong:${c['ink-strong']};--a-body:${c['ink-body']};
  --a-mute:${c['ink-mute']};--a-ok:${c.ok};--a-warn:${c.warn};
  --a-key:${c['syn-key']};--a-str:${c['syn-str']};--a-num:${c['syn-num']};
  --a-ui:${f.ui.join(',')};--a-mono:${f.mono.join(',')};
}`;
document.head.appendChild(style);
