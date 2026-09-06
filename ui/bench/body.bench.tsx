import { bench, describe } from "vitest";
import { render } from "@testing-library/react";
import { byteLength, bodyText, decodeBody, TEXT_MAX } from "../src/body";
import { FormRenderer } from "../src/components/FormRenderer";
import { TextRenderer } from "../src/components/TextRenderer";
import { formBody, textBody } from "./fixtures";

// The cost lives on the over-cap path only — the path the caps exist to bound,
// where the body may be the whole 10 MiB capture cap.
const OVER_CAP = textBody(10 * 1024 * 1024);
const FORM = formBody(40_000);

// The cut caps what is drawn, so a bigger body adds no DOM — only more bytes to
// count. It is the size at which counting them could possibly matter.
const FORM_OVER_CAP = formBody(470_000);

const WHOLE_BYTES = new TextEncoder().encode(OVER_CAP);

// Same length, one byte of it unreadable — enough to fail the strict pass and
// send the body through a second, lenient one.
const UNREADABLE_BYTES = WHOLE_BYTES.slice();
UNREADABLE_BYTES[UNREADABLE_BYTES.length - 1] = 0xff;

// Reporting whether a decode was whole costs a strict pass instead of a lenient
// one for a body that is, and a second pass for a body that is not. Both are
// priced, because the fix that reads them was made on the claim that the first
// is free.
describe("decodeBody — one decode, or two", () => {
  bench("10 MiB, whole", () => {
    decodeBody(WHOLE_BYTES);
  });

  bench("10 MiB, one unreadable byte", () => {
    decodeBody(UNREADABLE_BYTES);
  });

  bench("10 MiB, the lenient decode alone — what it replaced", () => {
    bodyText(WHOLE_BYTES);
  });
});

describe("byteLength — the size a cut marker states", () => {
  bench("10 MiB body", () => {
    byteLength(OVER_CAP);
  });

  bench(`the shown slice alone (${TEXT_MAX} chars)`, () => {
    byteLength(OVER_CAP.slice(0, TEXT_MAX));
  });
});

// BodyViewer holds the fetched bytes, so it knows the total size without
// deriving it. TextRenderer is benched both as it calls them and as a caller
// that cannot say.
const OVER_CAP_BYTES = byteLength(OVER_CAP);


describe("the character-rendered members over an over-cap body", () => {
  bench("TextRenderer — 10 MiB, as BodyViewer renders it", () => {
    render(<TextRenderer text={OVER_CAP} size={OVER_CAP_BYTES} />).unmount();
  });

  bench("TextRenderer — 10 MiB, size not supplied", () => {
    render(<TextRenderer text={OVER_CAP} />).unmount();
  });

  // Both sizes draw the same capped DOM and differ only in bytes to count, and
  // they cost the same — which is why FormRenderer was left holding its own
  // byte count while TextRenderer was handed one.
  bench("FormRenderer — 40k pairs", () => {
    render(<FormRenderer text={FORM} />).unmount();
  });

  bench("FormRenderer — 10 MiB", () => {
    render(<FormRenderer text={FORM_OVER_CAP} />).unmount();
  });
});
