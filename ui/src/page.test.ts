import { expect, test } from "vitest";
import { ABOUT_PAGE, PAGE_GLOBAL, SETTINGS_PAGE, pageOf } from "./page";

test("a window the shell marked as Settings renders the Settings page", () => {
  expect(pageOf({ [PAGE_GLOBAL]: SETTINGS_PAGE })).toBe("settings");
});

test("an unmarked or unknown window renders the app", () => {
  expect(pageOf({})).toBe("app");
  expect(pageOf({ [PAGE_GLOBAL]: "elsewhere" })).toBe("app");
});

test("a window the shell marked as About renders the About window", () => {
  expect(pageOf({ [PAGE_GLOBAL]: ABOUT_PAGE })).toBe("about");
});
