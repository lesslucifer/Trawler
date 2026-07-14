import { fireEvent, render, screen } from "@testing-library/react";
import { useReducer } from "react";
import { expect, test, vi } from "vitest";
import { Onboarding } from "./Onboarding";
import { COPY, reduce, type OnbState } from "./onboardingMachine";
import { guidedSteps, proxySnippet, proxySteps } from "./setup";

vi.mock("./setup", async (importOriginal) => {
  const mod = await importOriginal<typeof import("./setup")>();
  return { ...mod, guidedSteps: vi.fn(mod.guidedSteps), proxySteps: vi.fn(mod.proxySteps) };
});

const onboarding: OnbState = { phase: "onboarding", step: "trust-ca", everProven: false };
const atSetProxy: OnbState = { phase: "onboarding", step: "set-proxy", everProven: false };
const ADDR = "10.1.2.3:9999";

function Harness({
  start = onboarding,
  ...props
}: { start?: OnbState } & Partial<Pick<Parameters<typeof Onboarding>[0], "os" | "onInstallCa">>) {
  const [state, dispatch] = useReducer(reduce, start);
  return <Onboarding state={state} dispatch={dispatch} {...props} />;
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

test("an auto-capable host shows Install wired to onInstallCa, with guided steps alongside", async () => {
  const onInstallCa = vi.fn(async () => {});
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="macos" onInstallCa={onInstallCa} />);

  expect(screen.getByTestId("guided-steps")).toHaveTextContent(/security add-trusted-cert/);
  fireEvent.click(screen.getByTestId("install-ca"));
  expect(onInstallCa).toHaveBeenCalledOnce();
  expect(await screen.findByText(/Verify step will prove/)).toBeInTheDocument();
});

test("a failed install shows the error and marks the passed step failed, not done", async () => {
  const onInstallCa = vi.fn(() => Promise.reject(new Error("GET /ca.crt failed: 500")));
  render(<Harness start={onboarding} os="macos" onInstallCa={onInstallCa} />);

  fireEvent.click(screen.getByTestId("install-ca"));
  expect(await screen.findByText(/follow the steps below/)).toBeInTheDocument();
  expect(screen.getByTestId("install-ca-error")).toHaveTextContent("GET /ca.crt failed: 500");

  fireEvent.click(screen.getByTestId("onb-next"));
  expect(screen.getByTestId("step-trust-ca")).toHaveTextContent("✗ failed");
});

test("a non-auto host shows only the guided steps for its system", () => {
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="linux" />);
  expect(screen.queryByTestId("install-ca")).toBeNull();
  expect(screen.getByTestId("guided-steps")).toHaveTextContent(/update-ca-certificates/);
});

test("removal steps are shown for the selected system and switch with it", () => {
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="macos" onInstallCa={vi.fn()} />);
  expect(screen.getByTestId("removal-steps")).toHaveTextContent(/security delete-certificate/);
  fireEvent.change(screen.getByTestId("trust-target"), { target: { value: "windows" } });
  expect(screen.getByTestId("removal-steps")).toHaveTextContent(/certutil -delstore ROOT/);
});

test("selecting another system switches the steps and hides the auto install", () => {
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="macos" onInstallCa={vi.fn()} />);
  fireEvent.change(screen.getByTestId("trust-target"), { target: { value: "firefox" } });
  expect(screen.getByTestId("guided-steps")).toHaveTextContent(/own certificate store/);
  expect(screen.queryByTestId("install-ca")).toBeNull();
});

test("guided steps render as text, never markup", () => {
  vi.mocked(guidedSteps).mockReturnValueOnce(["<b>not bold</b> — literal text"]);
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="linux" />);
  expect(screen.getByText("<b>not bold</b> — literal text")).toBeInTheDocument();
});

test("the set-proxy step shows Set system proxy wired to onSetProxy, with the snippet alongside", () => {
  const onSetProxy = vi.fn(async () => {});
  render(
    <Onboarding
      state={atSetProxy}
      dispatch={vi.fn()}
      os="macos"
      proxyAddr={ADDR}
      onSetProxy={onSetProxy}
    />,
  );
  expect(screen.getByTestId("proxy-steps")).toHaveTextContent(/networksetup -setwebproxy/);
  fireEvent.click(screen.getByTestId("set-proxy"));
  expect(onSetProxy).toHaveBeenCalledOnce();
});

test("changing the client selector switches the shown snippet and steps", () => {
  render(<Onboarding state={atSetProxy} dispatch={vi.fn()} os="macos" proxyAddr={ADDR} onSetProxy={vi.fn()} />);
  fireEvent.change(screen.getByTestId("proxy-target"), { target: { value: "terminal" } });
  expect(screen.getByTestId("proxy-steps")).toHaveTextContent(/HTTPS_PROXY=http:\/\/10\.1\.2\.3:9999/);
  expect(screen.queryByTestId("set-proxy")).toBeNull(); // auto button only on the detected host
});

test("Copy puts the exact snippet command on the clipboard", () => {
  const writeText = vi.fn();
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  render(<Onboarding state={atSetProxy} dispatch={vi.fn()} os="macos" proxyAddr={ADDR} onSetProxy={vi.fn()} />);
  fireEvent.click(screen.getByTestId("copy-snippet"));
  expect(writeText).toHaveBeenCalledWith(proxySnippet("macos", ADDR));
});

test("proxy steps render as text, never markup", () => {
  vi.mocked(proxySteps).mockReturnValueOnce(["<b>not bold</b> — literal"]);
  render(<Onboarding state={atSetProxy} dispatch={vi.fn()} os="linux" proxyAddr={ADDR} />);
  expect(screen.getByText("<b>not bold</b> — literal")).toBeInTheDocument();
});

test("a failed set-proxy shows the error, keeps the snippet, and marks the passed step failed", async () => {
  const onSetProxy = vi.fn(() => Promise.reject(new Error("couldn't set the proxy: denied")));
  const Harn = () => {
    const [state, dispatch] = useReducer(reduce, atSetProxy);
    return <Onboarding state={state} dispatch={dispatch} os="macos" proxyAddr={ADDR} onSetProxy={onSetProxy} />;
  };
  render(<Harn />);

  fireEvent.click(screen.getByTestId("set-proxy"));
  expect(await screen.findByText(/follow the steps below/)).toBeInTheDocument();
  expect(screen.getByTestId("set-proxy-error")).toHaveTextContent("couldn't set the proxy: denied");
  expect(screen.getByTestId("proxy-steps")).toHaveTextContent(/networksetup/);

  fireEvent.click(screen.getByTestId("onb-next"));
  expect(screen.getByTestId("step-set-proxy")).toHaveTextContent("✗ failed");
});

test("Back to flows shows only once ever proven", () => {
  const { rerender } = render(<Onboarding state={onboarding} dispatch={vi.fn()} />);
  expect(screen.queryByTestId("onb-exit")).toBeNull();
  rerender(<Onboarding state={{ ...onboarding, everProven: true }} dispatch={vi.fn()} />);
  expect(screen.getByTestId("onb-exit")).toBeInTheDocument();
});
