function detect(nav) {
  const ua = (nav.userAgent || '') + ' ' + (nav.platform || '');
  const data = nav.userAgentData;
  const os = ((data && data.platform) || nav.platform || '').toLowerCase();
  if ((data && data.mobile) || /android|iphone|ipad|ipod|cros/i.test(ua)) return null;
  if (os.includes('mac')) return nav.maxTouchPoints > 1 ? null : 'macos';
  const arch = /aarch64|arm64|armv8/i.test(ua) ? 'arm64' : 'x64';
  if (os.includes('win')) return 'windows-' + arch;
  if (os.includes('linux')) return 'linux-' + arch;
  return null;
}

function show(key) {
  const block = document.getElementById('dl-' + key);
  if (!block) return;
  const system = block.closest('.card');
  const file = block.querySelector('a.file');
  const first = system.querySelector('.first');
  for (const id of ['get', 'one-get']) document.getElementById(id).href = file.href;
  for (const id of ['get-os', 'one-os']) document.getElementById(id).textContent = system.querySelector('h3').textContent;
  for (const id of ['pick-file', 'one-file']) document.getElementById(id).textContent = file.textContent;
  for (const id of ['pick-first', 'one-first']) document.getElementById(id).replaceChildren(...first.cloneNode(true).childNodes);
  document.getElementById('pick').hidden = false;
  document.getElementById('one').hidden = false;
  const all = document.getElementById('all');
  all.open = false;
  all.querySelector('summary').textContent = 'Other systems';
}

const picked = detect(navigator);
if (picked) {
  show(picked);
  const data = navigator.userAgentData;
  if (picked !== 'macos' && data && data.getHighEntropyValues) {
    data.getHighEntropyValues(['architecture']).then((hint) => {
      if (hint.architecture === 'arm') show(picked.replace('x64', 'arm64'));
    }, () => {});
  }
}
