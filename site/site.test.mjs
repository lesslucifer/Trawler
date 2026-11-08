import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const script = readFileSync(new URL('site.js', import.meta.url), 'utf8');

const SYSTEMS = [
  ['macOS', ['macos']],
  ['Windows', ['windows-x64', 'windows-arm64']],
  ['Linux', ['linux-x64', 'linux-arm64']],
];
const FILLED = ['get', 'one-get', 'get-os', 'one-os', 'pick-file', 'one-file', 'pick-first', 'one-first'];

function node(fields = {}) {
  return {
    href: '',
    textContent: '',
    childNodes: [],
    ...fields,
    replaceChildren(...nodes) {
      this.childNodes = nodes;
    },
    cloneNode() {
      return node({ childNodes: [...this.childNodes] });
    },
  };
}

function page({ without = [] } = {}) {
  const ids = Object.fromEntries(FILLED.map((id) => [id, node()]));
  ids.get.href = '#download';
  ids['get-os'].textContent = 'macOS · Windows · Linux';
  ids.pick = node({ hidden: true });
  ids.one = node({ hidden: true });
  const summary = node({ textContent: 'All systems' });
  ids.all = node({ open: true, querySelector: (selector) => (selector === 'summary' ? summary : null) });
  for (const [system, keys] of SYSTEMS) {
    const parts = { h3: node({ textContent: system }), '.first': node({ childNodes: [`first open on ${system}`] }) };
    const card = { querySelector: (selector) => parts[selector] ?? null };
    for (const key of keys) {
      const file = node({ href: `releases/download/v0.1.0/${key}.file`, textContent: `${key}.file` });
      ids[`dl-${key}`] = {
        closest: (selector) => (selector === '.card' ? card : null),
        querySelector: (selector) => (selector === 'a.file' ? file : null),
      };
    }
  }
  for (const id of without) delete ids[id];
  const asked = [];
  const document = {
    getElementById(id) {
      asked.push(id);
      return ids[id] ?? null;
    },
  };
  return { ids, summary, document, blocks: () => asked.filter((id) => id.startsWith('dl-')) };
}

async function run(navigator, options) {
  const shown = page(options);
  vm.runInNewContext(script, { navigator, document: shown.document });
  await new Promise((settled) => setImmediate(settled));
  return shown;
}

const hint = (architecture) => () => Promise.resolve({ architecture });
const MAC_SAFARI = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15';
const MAC_CHROME = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36';
const WIN_EDGE = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0';
const LINUX_CHROME = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36';
const windowsData = (getHighEntropyValues) => ({ platform: 'Windows', mobile: false, getHighEntropyValues });

const CASES = [
  ['Safari on a Mac', { platform: 'MacIntel', maxTouchPoints: 0, userAgent: MAC_SAFARI }, ['dl-macos']],
  [
    'Chrome on a Mac',
    { platform: 'MacIntel', maxTouchPoints: 0, userAgent: MAC_CHROME, userAgentData: { platform: 'macOS', mobile: false, getHighEntropyValues: hint('arm') } },
    ['dl-macos'],
  ],
  ['an iPad asking for the desktop site', { platform: 'MacIntel', maxTouchPoints: 5, userAgent: MAC_SAFARI }, []],
  ['Edge on Windows x64', { platform: 'Win32', maxTouchPoints: 0, userAgent: WIN_EDGE, userAgentData: windowsData(hint('x86')) }, ['dl-windows-x64']],
  [
    'Edge on Windows on Arm',
    { platform: 'Win32', maxTouchPoints: 0, userAgent: WIN_EDGE, userAgentData: windowsData(hint('arm')) },
    ['dl-windows-x64', 'dl-windows-arm64'],
  ],
  [
    'Edge refusing the architecture hint',
    { platform: 'Win32', maxTouchPoints: 0, userAgent: WIN_EDGE, userAgentData: windowsData(() => Promise.reject(new Error('refused'))) },
    ['dl-windows-x64'],
  ],
  [
    'Firefox on Windows',
    { platform: 'Win32', maxTouchPoints: 0, userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:132.0) Gecko/20100101 Firefox/132.0' },
    ['dl-windows-x64'],
  ],
  [
    'Firefox on Linux x64',
    { platform: 'Linux x86_64', maxTouchPoints: 0, userAgent: 'Mozilla/5.0 (X11; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0' },
    ['dl-linux-x64'],
  ],
  [
    'Firefox on Linux arm64',
    { platform: 'Linux aarch64', maxTouchPoints: 0, userAgent: 'Mozilla/5.0 (X11; Linux aarch64; rv:132.0) Gecko/20100101 Firefox/132.0' },
    ['dl-linux-arm64'],
  ],
  [
    'Chrome on Linux arm64 behind a reduced x86_64 user agent',
    { platform: 'Linux aarch64', maxTouchPoints: 0, userAgent: LINUX_CHROME, userAgentData: { platform: 'Linux', mobile: false, getHighEntropyValues: hint('arm') } },
    ['dl-linux-arm64', 'dl-linux-arm64'],
  ],
  [
    'Chrome on Android',
    {
      platform: 'Linux armv81',
      maxTouchPoints: 5,
      userAgent: 'Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Mobile Safari/537.36',
      userAgentData: { platform: 'Android', mobile: true, getHighEntropyValues: hint('arm') },
    },
    [],
  ],
  [
    'Safari on an iPhone',
    { platform: 'iPhone', maxTouchPoints: 5, userAgent: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1' },
    [],
  ],
  [
    'ChromeOS, told apart from Linux by its user agent alone',
    {
      platform: 'Linux x86_64',
      maxTouchPoints: 0,
      userAgent: 'Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36',
    },
    [],
  ],
  ['a browser that reports nothing', {}, []],
];

for (const [name, navigator, blocks] of CASES) {
  test(`${name} asks for ${blocks.join(' then ') || 'no download block'}`, async () => {
    assert.deepEqual((await run(navigator)).blocks(), blocks);
  });
}

test('a detected system fills both buttons from its file and folds the full list under Other systems', async () => {
  const { ids, summary } = await run(CASES[3][1]);
  const file = 'releases/download/v0.1.0/windows-x64.file';
  assert.equal(ids.get.href, file);
  assert.equal(ids['one-get'].href, file);
  assert.equal(ids['get-os'].textContent, 'Windows');
  assert.equal(ids['one-os'].textContent, 'Windows');
  assert.equal(ids['pick-file'].textContent, 'windows-x64.file');
  assert.equal(ids['one-file'].textContent, 'windows-x64.file');
  assert.deepEqual(ids['pick-first'].childNodes, ['first open on Windows']);
  assert.deepEqual(ids['one-first'].childNodes, ['first open on Windows']);
  assert.equal(ids.pick.hidden, false);
  assert.equal(ids.one.hidden, false);
  assert.equal(ids.all.open, false);
  assert.equal(summary.textContent, 'Other systems');
});

test('the architecture hint moves both buttons to the arm64 file', async () => {
  const { ids } = await run(CASES[4][1]);
  assert.equal(ids.get.href, 'releases/download/v0.1.0/windows-arm64.file');
  assert.equal(ids['one-get'].href, 'releases/download/v0.1.0/windows-arm64.file');
  assert.equal(ids['one-file'].textContent, 'windows-arm64.file');
});

test('an unknown system leaves the page as it is without the script', async () => {
  const { ids, summary } = await run(CASES[10][1]);
  assert.equal(ids.get.href, '#download');
  assert.equal(ids['get-os'].textContent, 'macOS · Windows · Linux');
  assert.equal(ids.one.hidden, true);
  assert.equal(ids.pick.hidden, true);
  assert.equal(ids.all.open, true);
  assert.equal(summary.textContent, 'All systems');
});

test('a missing download block leaves the page unchanged', async () => {
  const { ids, summary } = await run(CASES[3][1], { without: ['dl-windows-x64'] });
  assert.equal(ids.get.href, '#download');
  assert.equal(ids.one.hidden, true);
  assert.equal(ids.all.open, true);
  assert.equal(summary.textContent, 'All systems');
});
