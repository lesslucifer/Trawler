import { fireEvent, render, screen } from "@testing-library/react";
import { useReducer } from "react";
import { expect, test, vi } from "vitest";
import { Onboarding } from "./Onboarding";
import { COPY, reduce, type OnbState } from "./onboardingMachine";

const onboarding: OnbState = { phase: "onboarding", step: "trust-ca", everProven: false };

function Harness({ start = onboarding }: { start?: OnbState }) {
  const [state, dispatch] = useReducer(reduce, start);
  return <Onboarding state={state} dispatch={dispatch} />;
}

test("renders all three steps and the active step's body", () => {
  render(<Onboarding state={{ ...onboarding, step: "verify" }} dispatch={vi.fn()} />);
  expect(screen.getByText(/Trust the CA/)).toBeInTheDocument();
  expect(screen.getByText(/Set the proxy/)).toBeInTheDocument();
  expect(screen.getByText(/Verify/)).toBeInTheDocument();
  expect(screen.getByTestId("step-body")).toHaveTextContent(COPY.steps.verify.body);
});

test("Next moves the active step forward", () => {
  render(<Harness />);
  expect(screen.getByTestId("step-trust-ca")).toHaveAttribute("data-status", "active");
  fireEvent.click(screen.getByTestId("onb-next"));
  expect(screen.getByTestId("step-set-proxy")).toHaveAttribute("data-status", "active");
  expect(screen.getByTestId("step-trust-ca")).toHaveAttribute("data-status", "done");
});

test("Back to flows shows only once ever proven", () => {
  const { rerender } = render(<Onboarding state={onboarding} dispatch={vi.fn()} />);
  expect(screen.queryByTestId("onb-exit")).toBeNull();
  rerender(<Onboarding state={{ ...onboarding, everProven: true }} dispatch={vi.fn()} />);
  expect(screen.getByTestId("onb-exit")).toBeInTheDocument();
});
