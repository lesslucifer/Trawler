import { render, screen } from "@testing-library/react";
import App from "./App";

test("app mounts and renders the placeholder view", () => {
  render(<App />);
  expect(screen.getByTestId("placeholder").textContent).toContain("Trawler");
});
