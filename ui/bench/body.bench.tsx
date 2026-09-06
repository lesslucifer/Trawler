import { bench, describe } from "vitest";
import { render } from "@testing-library/react";
import { byteLength, TEXT_MAX } from "../src/body";
import { FormRenderer } from "../src/components/FormRenderer";
import { TextRenderer } from "../src/components/TextRenderer";
import { formBody, textBody } from "./fixtures";

// The cost lives on the over-cap path only — the path the caps exist to bound,
// where the body may be the whole 10 MiB capture cap.
const OVER_CAP = textBody(10 * 1024 * 1024);
const FORM = formBody(40_000);

describe("byteLength — the size a cut marker states", () => {
  bench("10 MiB body", () => {
    byteLength(OVER_CAP);
  });

  bench(`the shown slice alone (${TEXT_MAX} chars)`, () => {
    byteLength(OVER_CAP.slice(0, TEXT_MAX));
  });
});

describe("the character-rendered members over an over-cap body", () => {
  bench("TextRenderer — 10 MiB", () => {
    render(<TextRenderer text={OVER_CAP} />).unmount();
  });

  bench("FormRenderer — 40k pairs", () => {
    render(<FormRenderer text={FORM} />).unmount();
  });
});
