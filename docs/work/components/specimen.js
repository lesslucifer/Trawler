/* Trawler — shared specimen/design-run behaviours (E-DFLOW)
 *
 * The two interactions a design doc needs, declared in markup instead of
 * re-implemented per doc. Classic script (not a module) so it loads over
 * file:// like the rest of the doc assets:
 *
 *   design run   <script src="components/specimen.js" defer></script>
 *   design file  <script src="specimen.js" defer></script>
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
 */
(function () {
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
})();
