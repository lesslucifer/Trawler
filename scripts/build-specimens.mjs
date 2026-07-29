/* Trawler — compile the app's collection into what a design doc can load (E-DFLOW)
 *
 *   cargo xtask ui specimens        (or: node scripts/build-specimens.mjs)
 *
 * Writes ONE generated, git-ignored classic script beside this file:
 *
 *   specimens.bundle.js   the app's Tailwind theme, then the shared specimen
 *                         behaviours, then the real ui/src/components/ + React
 *                         as window.Trawler
 *
 * Three things in one file because a doc needs all three and their order is a
 * constraint, not a preference (theme before the components that use it). One
 * <script> cannot be loaded in the wrong order.
 *
 * WHY A BUILD STEP, when the whole point was not having one:
 *
 * A design doc must open by double-click. On a file:// origin a page may not read
 * any local file -- fetch, sync XHR and async XHR are all refused -- and
 * <script type="module"> never executes, because module loading is CORS-governed
 * and a file:// origin is opaque. So the run-time loader that read ui/src/
 * directly could not work there; it needed a server, which is worse than a build.
 *
 * A classic <script src="./x.js"> is NOT CORS-restricted and DOES execute from
 * file://. That is the whole workaround: move the compile from load time to
 * author time, and the doc needs nothing at run time.
 *
 * This is still not a copy. Nobody writes or edits these files, they are
 * derived from ui/src/ on demand, and they are git-ignored so they cannot rot in
 * a review: a stale bundle is a re-run, never a divergence to reconcile. The
 * invariant holds -- the specimen IS the component -- with "when" changed.
 *
 * esbuild comes with Vite, already in ui/package.json. No new dependency.
 */
import { execFile } from 'node:child_process';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const run = promisify(execFile);
const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..');
const UI = join(REPO, 'ui');
const ASSETS = join(REPO, 'doc-assets/components');
const ESBUILD = join(UI, 'node_modules/.bin/esbuild');

/* Order is the reason these share one entry:
 *
 *   theme   must set window.tailwind BEFORE the Play CDN compiles, and AFTER the
 *           CDN tag replaces window.tailwind on load -- so: CDN tag, then us.
 *   specimen.js  imported for side effects, from its own file: it stays the
 *           editable source, still loaded standalone by every unmigrated design
 *           run (verify-doc requires that exact src), so there is no second copy.
 *   collection   re-exports React too, which keeps the doc free of an import map:
 *           window.Trawler.React is the only React instance on the page.
 */
const BUNDLE_ENTRY = `
import cfg from "../tailwind.config.js";

const tw = window.tailwind ?? (window.tailwind = {});
tw.config = { theme: cfg.theme };

const { colors: c, fontFamily: f } = cfg.theme.extend;
const style = document.createElement("style");
style.textContent = \`.app{
  --a-canvas:\${c.canvas};--a-soft:\${c["canvas-soft"]};--a-line:\${c.hairline};
  --a-ink:\${c.ink};--a-strong:\${c["ink-strong"]};--a-body:\${c["ink-body"]};
  --a-mute:\${c["ink-mute"]};--a-ok:\${c.ok};--a-warn:\${c.warn};
  --a-key:\${c["syn-key"]};--a-str:\${c["syn-str"]};--a-num:\${c["syn-num"]};
  --a-ui:\${f.ui.join(",")};--a-mono:\${f.mono.join(",")};
}\`;
document.head.appendChild(style);

import "../../doc-assets/components/specimen.js";

export * from "../src/components/index";
export * as React from "react";
export { createRoot } from "react-dom/client";
`;

async function build(entrySource, extension, outfile, extra = []) {
  /* The entry must sit inside ui/ so bare imports resolve against its
     node_modules; a temp dir elsewhere cannot see react. */
  const scratch = await mkdtemp(join(UI, '.specimens-'));
  const entry = join(scratch, `entry${extension}`);
  await writeFile(entry, entrySource);
  try {
    const { stderr } = await run(ESBUILD, [
      entry, '--bundle', '--format=iife', '--jsx=automatic', '--minify',
      `--define:process.env.NODE_ENV="development"`,
      `--outfile=${outfile}`, ...extra,
    ], { cwd: UI });
    return stderr.trim().split('\n').pop();
  } finally {
    await rm(scratch, { recursive: true, force: true });
  }
}

const bundle = join(ASSETS, 'specimens.bundle.js');

console.log('bundle →', await build(BUNDLE_ENTRY, '.tsx', bundle, ['--global-name=Trawler']));
console.log('\nwrote doc-assets/components/specimens.bundle.js — re-run after changing ui/src/components/ or specimen.js');
