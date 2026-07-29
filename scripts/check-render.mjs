/* Trawler — the render gate for migrated design files (E-DFLOW)
 *
 *   cargo xtask ui specimens --check
 *   node scripts/check-render.mjs [file.html …]   (single file)
 *
 * A migrated specimen fails SILENTLY: the component throws, the mount stays
 * empty, and an empty div beside correct prose reads as "no specimen here"
 * rather than "broken". Every real failure found while building this looked
 * exactly like that — a TS `as` cast Babel rejects, a wrong module path, a
 * file:// origin that blocks every read. So the gate asserts the two things a
 * reader cannot: a clean console -- thrown errors, rejected promises AND anything
 * console.error/warn reports -- and every specimen mount non-empty.
 *
 * Drives the user's own Chrome headless (no Playwright dependency).
 *
 * Opens each doc over file:// with NO flags -- exactly how a reader double-clicks
 * it. Two earlier versions of this gate tested something easier than reality: one
 * passed --allow-file-access-from-files, the other served over HTTP. Both went
 * green while the docs were blank when opened normally. A gate must not grant
 * itself privileges the reader does not have.
 */
import { execFile } from 'node:child_process';
import { existsSync } from 'node:fs';
import { readdir, readFile, writeFile, rm } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const run = promisify(execFile);
const HERE = dirname(fileURLToPath(import.meta.url));
const DOCS = join(HERE, '../docs');
/* macOS-primary, like the rest of the UI tooling (docs/testing-strategy.html),
   but overridable: without CHROME the default path is another "works on the
   author's machine" trap, and a bare spawn ENOENT does not say what to install. */
const CHROME = process.env.CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';

if (!existsSync(CHROME)) {
  console.error(`no Chrome at ${CHROME}\nset CHROME=/path/to/chrome (any Chromium with --headless works)`);
  process.exit(1);
}

const PROBE = `<script>
  (function poll(tries) {
    /* spec-log is an output sink a click fills, not a mount — legitimately empty
       on load. Everything else named spec-* must have rendered something. */
    const mounts = [...document.querySelectorAll('[id^="spec-"], [id$="-demo"]')]
      .filter((m) => m.id !== 'spec-log');
    const pending = mounts.filter((m) => m.children.length === 0 && !m.textContent.trim());
    if (pending.length && tries < 120) return setTimeout(() => poll(tries + 1), 100);
    const report = {
      mounts: mounts.length,
      empty: pending.map((m) => m.id),
      errors: window.__gateErrors,
      buttons: document.querySelectorAll('button').length,
      forced: document.querySelector('[data-forced-states]')?.dataset.forcedStates ?? null,
    };
    const box = document.createElement('pre');
    box.id = 'GATE';
    box.textContent = 'GATE ' + JSON.stringify(report);
    document.body.appendChild(box);
  })(0);
</script>`;

/* console.error must be wrapped, not just listened for: React reports a bad key,
   a failed prop type and a caught-then-logged render error through console.error
   alone, which fires no 'error' event. A gate that only listens sees errors: []
   on a doc whose console is full -- that gap let a drifted hand-drawn specimen
   sit in copy-text.html through a green run. */
const CAPTURE = `<script>
  window.__gateErrors = [];
  window.addEventListener('error', (e) => {
    window.__gateErrors.push(String(e.message || e.error));
  }, true);
  window.addEventListener('unhandledrejection', (e) => {
    window.__gateErrors.push('rejection: ' + (e.reason?.message ?? e.reason));
  });
  /* The Play CDN and the in-browser Babel transformer each greet every load with
     a "not for production" banner. Both are deliberate here -- a design file is
     the one place they are correct -- so they are named exactly and ignored;
     anything else on console.error/warn fails the gate. */
  const EXPECTED = ['cdn.tailwindcss.com should not be used in production',
                    'You are using the in-browser Babel transformer'];
  for (const level of ['error', 'warn']) {
    const original = console[level].bind(console);
    console[level] = (...args) => {
      const text = args.map((a) => a?.message ?? String(a)).join(' ');
      if (!EXPECTED.some((allowed) => text.includes(allowed))) {
        window.__gateErrors.push(level + ': ' + text);
      }
      original(...args);
    };
  }
</script>`;

async function check(file) {
  const source = await readFile(file, 'utf8');
  if (!source.includes('specimens.bundle.js')) return { file, skipped: 'not migrated' };

  const instrumented = source
    .replace('<head>', '<head>\n' + CAPTURE)
    .replace('</body>', PROBE + '\n</body>');
  const temp = join(dirname(file), `_gate-${Date.now()}.html`);
  await writeFile(temp, instrumented);

  try {
    const { stdout } = await run(CHROME, [
      '--headless', '--disable-gpu',
      '--virtual-time-budget=30000', '--dump-dom', 'file://' + resolve(temp),
    ], { maxBuffer: 64 * 1024 * 1024 });
    const match = stdout.match(/<pre id="GATE">GATE (.*?)<\/pre>/s);
    if (!match) return { file, fail: 'the page never reported — no JS ran at all' };
    const report = JSON.parse(match[1].replace(/&quot;/g, '"').replace(/&amp;/g, '&'));
    const problems = [];
    if (report.errors.length) problems.push(`console: ${report.errors.join(' | ')}`);
    if (report.empty.length) problems.push(`empty mounts: ${report.empty.join(', ')}`);
    if (!report.mounts) problems.push('loads the bundle but has no spec-* mount to fill');
    return { file, report, fail: problems.length ? problems.join('; ') : null };
  } finally {
    await rm(temp, { force: true });
  }
}

const args = process.argv.slice(2);
const files = args.length
  ? args
  : await htmlFilesUnder(DOCS);

async function htmlFilesUnder(dir) {
  const found = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) found.push(...(await htmlFilesUnder(full)));
    else if (entry.name.endsWith('.html')) found.push(full);
  }
  return found.sort();
}

let failed = 0;
for (const file of files) {
  const result = await check(file);
  const name = result.file.split('/').pop();
  if (result.skipped) continue;
  if (result.fail) {
    failed += 1;
    console.log(`FAIL  ${name}\n      ${result.fail}`);
  } else {
    /* The forced-rule count is a snapshot taken while Tailwind is still adding
       passes, so it is progress information, not an assertion. */
    const { mounts, buttons, forced } = result.report;
    console.log(`ok    ${name}  (${mounts} mounts, ${buttons} buttons, forced rules ≥${forced ?? 0})`);
  }
}
console.log(failed ? `\n${failed} file(s) failed` : '\nall migrated design files render');
process.exit(failed ? 1 : 0);
