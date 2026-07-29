/* Trawler — shared specimen/design-run behaviours (E-DFLOW)
 *
 * The two interactions a design doc needs, declared in markup instead of
 * re-implemented per doc. Classic script (not a module) so it loads over
 * file:// like the rest of the doc assets:
 *
 *   design run   <script src="../../doc-assets/components/specimen.js" defer></script>
 *   design file  folded into specimens.bundle.js (build-specimens.mjs imports
 *                this file for its side effects -- one source, no copy)
 *
 * Both entry points reach the same code at different times: `defer` runs after
 * the DOM, the bundle runs in <head> before it. Hence the ready-guard below --
 * without it the bundled path queries an empty document and binds nothing.
 *
 * Keyed highlight map — trace one component across a tree and a catalog:
 *   <div data-map>… <span data-key="body-viewer">BodyViewer</span> …
 *                   <tr data-key="body-viewer">…</tr> …</div>
 *
 * State switch — flip a mockup through the states the Interaction table named:
 *   <div class="mock" id="mock">…</div>
 *   <button data-state-toggle="mock" data-state-class="inflight"
 *           data-label-off="▶ show in-flight" data-label-on="◀ show complete">
 *
 * Visibility inside a switched mockup is CSS, not script: .<class> .x-only
 * hides what the other state owns (see the run doc's assembly block).
 *
 * The highlight-map styling below came from specimen.css, deleted in W5 step 3
 * when the specimens moved to Tailwind. It is doc interaction, not component
 * contract, so it travels with the behaviour it belongs to instead.
 *
 * Driving a state that has no prop — window.specimenReach(host, {type}):
 *   a swatch cannot set `open` or `confirming` from outside, because both are
 *   internal useState. So it does what a user does: wait for React to commit,
 *   then really click, and optionally really type. Shared because copy-text and
 *   select each grew their own copy of this poll-then-click loop.
 */
(function () {
  const TICK = 50;
  const LIMIT = 60;

  function pollFor(selector, host, then) {
    let tries = 0;
    const timer = setInterval(() => {
      const found = host.querySelector(selector);
      if (!found && ++tries < LIMIT) return;
      clearInterval(timer);
      if (found) then(found);
    }, TICK);
  }

  /* React commits asynchronously, so the element is absent on the first tick;
     the poll is bounded so a wrong id cannot spin forever. */
  window.specimenReach = (host, { type } = {}) => {
    if (!host) return host;
    pollFor('button, [role="button"]', host, (target) => {
      target.click();
      if (type === undefined) return;
      pollFor('input', host, (field) => {
        const native = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value');
        native.set.call(field, type);
        field.dispatchEvent(new Event('input', { bubbles: true }));
      });
    });
    return host;
  };
})();
function installSpecimenBehaviours() {
  const DOC_STYLE = `
[data-map] [data-key]{cursor:pointer;border-radius:3px;transition:background .1s;}
[data-map] pre [data-key]{padding:0 3px;}
[data-map] pre [data-key]:hover{background:var(--line);}
[data-map] pre [data-key].hl{background:color-mix(in srgb, var(--accent) 24%, transparent);color:var(--ink);font-weight:600;}
[data-map] tbody tr[data-key]:hover td{background:var(--line);}
[data-map] tbody tr[data-key].hl td{background:color-mix(in srgb, var(--accent) 16%, transparent);}
`;
  const style = document.createElement('style');
  style.textContent = DOC_STYLE;
  document.head.appendChild(style);

  document.querySelectorAll('[data-map]').forEach((map) => {
    const keyed = map.querySelectorAll('[data-key]');
    const clear = () => keyed.forEach((el) => el.classList.remove('hl'));
    keyed.forEach((el) => {
      el.addEventListener('click', (event) => {
        event.stopPropagation();
        const alreadyLit = el.classList.contains('hl');
        clear();
        if (!alreadyLit) {
          keyed.forEach((other) => other.classList.toggle('hl', other.dataset.key === el.dataset.key));
        }
      });
    });
    document.addEventListener('click', clear);
  });

  document.querySelectorAll('[data-state-toggle]').forEach((button) => {
    const target = document.getElementById(button.dataset.stateToggle);
    if (!target) return;
    button.addEventListener('click', () => {
      const on = target.classList.toggle(button.dataset.stateClass);
      button.textContent = on ? button.dataset.labelOn : button.dataset.labelOff;
    });
  });

  /* Forced interaction states, for state swatches of a LIVE component.
   *
   *   <div class="swatch force-hover">…a real <Button>…</Button>…</div>
   *
   * A swatch cannot hold a pointer, and hand-writing `color:var(--a-ink)` to
   * fake one re-introduces exactly the copy this migration removes. So instead
   * of declaring anything, this copies the rules Tailwind ALREADY generated for
   * the component's own hover:/focus-visible:/active: utilities and re-targets
   * them at .force-* wrappers. Values come from the component's class string in
   * both paths, so a forced swatch cannot disagree with a real hover.
   *
   * Runs after the CDN's first pass; harmless if a doc uses no .force-* class.
   */
  const FORCED = { 'force-hover': ':hover', 'force-focus': ':focus-visible', 'force-active': ':active' };

  let mirrored = document.createElement('style');

  function mirrorStateRules() {
    const seen = new Set();
    const out = [];
    for (const sheet of document.styleSheets) {
      let rules;
      try { rules = sheet.cssRules; } catch { continue; }
      for (const rule of rules) {
        if (!rule.selectorText || rule.parentStyleSheet?.ownerNode === mirrored) continue;
        for (const [wrapper, pseudo] of Object.entries(FORCED)) {
          /* Only a Tailwind state utility qualifies: its class name literally
             contains the variant prefix (`.active\:bg-wash-tint:active`). A rule
             that merely ends in :hover — the doc's own highlight-map styling
             above, say — is not a component state and must not be forced on. */
          if (!rule.selectorText.endsWith(pseudo)) continue;
          const base = rule.selectorText.slice(0, -pseudo.length);
          if (!base.includes(`.${pseudo.slice(1)}\\:`)) continue;
          const forced = `.${wrapper} ${base}{${rule.style.cssText}}`;
          if (seen.has(forced)) continue;
          seen.add(forced);
          out.push(forced);
        }
      }
    }
    if (!out.length) return false;
    mirrored.dataset.forcedStates = String(out.length);
    mirrored.textContent = out.join('\n');
    if (!mirrored.isConnected) document.head.appendChild(mirrored);
    return true;
  }

  if (document.querySelector('.force-hover, .force-focus, .force-active')) {
    /* The Play CDN regenerates its sheet on every DOM mutation, so the rules for
       a React-rendered component exist only after a later pass. Poll a bounded
       number of times rather than observing: an observer on <head> or the tree
       sees its OWN inserted <style> and re-triggers itself forever. */
    /* Re-mirror on a fixed schedule for ~8s. The Play CDN rebuilds its sheet in
       several passes as React mounts — measured on this page: 105 rules at 1s,
       183 (the `active:` variants among them) only at ~5s. Any "settled" check
       finishes during the plateau before that second pass and permanently misses
       them, so wall-clock coverage is what matters, not stability. Mirroring is
       idempotent and deduped, so extra passes cost nothing. */
    let elapsed = 0;
    const tick = () => {
      mirrorStateRules();
      elapsed += 500;
      if (elapsed <= 8000) setTimeout(tick, 500);
    };
    tick();
  }
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', installSpecimenBehaviours, { once: true });
} else {
  installSpecimenBehaviours();
}
