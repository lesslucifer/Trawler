import "@testing-library/jest-dom/vitest";

// jsdom lays nothing out and its Range has no client rects; CodeMirror's first
// measure pass asks for them on the frame after mount, and a test that unmounts
// before that frame lets the TypeError escape as an unhandled error.
const emptyRect = () => ({ x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0, toJSON() {} }) as DOMRect;
Range.prototype.getClientRects ??= () => [] as unknown as DOMRectList;
Range.prototype.getBoundingClientRect ??= emptyRect;

// jsdom has no PointerEvent, so a fired pointer event falls back to a bare Event
// and loses clientX, clientY and button.
globalThis.PointerEvent ??= class extends MouseEvent {
  readonly pointerId: number;
  constructor(type: string, init: PointerEventInit = {}) {
    super(type, init);
    this.pointerId = init.pointerId ?? 0;
  }
} as unknown as typeof PointerEvent;
