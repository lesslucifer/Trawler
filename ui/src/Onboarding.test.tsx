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

// Select is a listbox, not a native <select>: open the trigger, pick the option.
const choose = (picker: string, option: string | RegExp) => {
  fireEvent.click(screen.getByRole("button", { name: picker }));
  fireEvent.click(screen.getByRole("option", { name: option }));
};

test("renders all three steps and the active step's body", () => {
  render(<Onboarding state={{ ...onboarding, step: "verify" }} dispatch={vi.fn()} />);
  expect(screen.getByText(/trust the CA/)).toBeInTheDocument();
  expect(screen.getByText(/set the proxy/)).toBeInTheDocument();
  expect(screen.getByText(/make a request/)).toBeInTheDocument();
  expect(screen.getByTestId("step-body")).toHaveTextContent(COPY.steps.verify.body);
});

// The checklist is re-entered out of order, so a header click is the whole
// navigation — there is no prev/next pair to make it a wizard.
test("clicking a step's header opens it, and only one body is open at a time", () => {
  render(<Harness />);
  expect(screen.getByTestId("step-trust-ca")).toHaveAttribute("data-status", "active");

  fireEvent.click(screen.getByText(/2 · set the proxy/));
  expect(screen.getByTestId("step-set-proxy")).toHaveAttribute("data-status", "active");
  expect(screen.getAllByTestId("step-body")).toHaveLength(1);
});

test("no Back/Next pair — a checklist that ships prev/next is claiming to be a wizard", () => {
  render(<Harness />);
  expect(screen.queryByTestId("onb-next")).toBeNull();
  expect(screen.queryByTestId("onb-back")).toBeNull();
});

test("an auto-capable host shows install wired to onInstallCa, with guided steps behind the manual disclosure", async () => {
  const onInstallCa = vi.fn(async () => {});
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="macos" onInstallCa={onInstallCa} />);

  // P3: the one-click action is primary; the per-OS steps live behind a disclosure.
  expect(screen.getByTestId("manual")).toBeInTheDocument();
  expect(screen.getByTestId("guided-steps")).toHaveTextContent(/security add-trusted-cert/);
  fireEvent.click(screen.getByRole("button", { name: "install certificate" }));
  expect(onInstallCa).toHaveBeenCalledOnce();
  expect(await screen.findByText(/we'll confirm it on your first flow/)).toBeInTheDocument();
});

test("a failed install shows the error and marks the step warn in its status line, not a false done", async () => {
  const onInstallCa = vi.fn(() => Promise.reject(new Error("GET /ca.crt failed: 500")));
  render(<Harness start={onboarding} os="macos" onInstallCa={onInstallCa} />);

  fireEvent.click(screen.getByRole("button", { name: "install certificate" }));
  expect(await screen.findByText(/use the manual steps below/)).toBeInTheDocument();
  expect(screen.getByTestId("install-ca-status")).toHaveTextContent("GET /ca.crt failed: 500");

  const status = screen.getByTestId("step-trust-ca-status");
  expect(status).toHaveTextContent(/install failed/);
  expect(status).not.toHaveTextContent(/trusted on this machine/);
});

test("a failed install force-opens the manual route it tells the user to use", async () => {
  const onInstallCa = vi.fn(() => Promise.reject(new Error("declined")));
  render(<Harness start={onboarding} os="macos" onInstallCa={onInstallCa} />);

  expect(screen.getByTestId("manual").querySelector("details")).not.toHaveAttribute("open");
  fireEvent.click(screen.getByRole("button", { name: "install certificate" }));

  expect(await screen.findByText(/use the manual steps below/)).toBeInTheDocument();
  expect(screen.getByTestId("manual").querySelector("details")).toHaveAttribute("open");
});

test("a non-auto host shows the guided steps inline, no disclosure to bury them", () => {
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="linux" />);
  expect(screen.queryByRole("button", { name: "install certificate" })).toBeNull();
  expect(screen.queryByTestId("manual")).toBeNull();
  expect(screen.getByTestId("guided-steps")).toHaveTextContent(/update-ca-certificates/);
});

test("removal steps are shown for the selected system and switch with it", () => {
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="macos" onInstallCa={vi.fn()} />);
  expect(screen.getByTestId("removal-steps")).toHaveTextContent(/security delete-certificate/);
  choose("trust target", "Windows");
  expect(screen.getByTestId("removal-steps")).toHaveTextContent(/certutil -delstore ROOT/);
});

test("selecting another system switches the steps and hides the auto install", () => {
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="macos" onInstallCa={vi.fn()} />);
  choose("trust target", "Firefox");
  expect(screen.getByTestId("guided-steps")).toHaveTextContent(/own certificate store/);
  expect(screen.queryByRole("button", { name: "install certificate" })).toBeNull();
});

test("guided steps render as text, never markup", () => {
  vi.mocked(guidedSteps).mockReturnValueOnce(["<b>not bold</b> — literal text"]);
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="linux" />);
  expect(screen.getByText("<b>not bold</b> — literal text")).toBeInTheDocument();
});

test("the set-proxy step shows the action wired to onSetProxy, with the snippet alongside", () => {
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
  fireEvent.click(screen.getByRole("button", { name: "set system proxy" }));
  expect(onSetProxy).toHaveBeenCalledOnce();
});

test("changing the client selector switches the shown snippet and steps", () => {
  render(<Onboarding state={atSetProxy} dispatch={vi.fn()} os="macos" proxyAddr={ADDR} onSetProxy={vi.fn()} />);
  choose("proxy target", /Terminal/);
  expect(screen.getByTestId("proxy-steps")).toHaveTextContent(/HTTPS_PROXY=http:\/\/10\.1\.2\.3:9999/);
  expect(screen.queryByRole("button", { name: "set system proxy" })).toBeNull(); // auto button only on the detected host
});

// The two "copy command" buttons are retired: CopyText makes the command itself
// the target and confirms at the value, where the old calls were unguarded and
// silent on rejection.
test("the proxy command copies through CopyText, confirming at the value", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  render(<Onboarding state={atSetProxy} dispatch={vi.fn()} os="macos" proxyAddr={ADDR} onSetProxy={vi.fn()} />);

  fireEvent.click(screen.getByRole("button", { name: /proxy command/ }));
  expect(writeText).toHaveBeenCalledWith(proxySnippet("macos", ADDR));
  expect(await screen.findByText("copied")).toBeInTheDocument();
});

test("a clipboard that rejects says so instead of failing silently", async () => {
  const writeText = vi.fn().mockRejectedValue(new Error("denied"));
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  render(<Onboarding state={{ ...onboarding, step: "verify" }} dispatch={vi.fn()} proxyAddr={ADDR} />);

  fireEvent.click(screen.getByRole("button", { name: /prove-it command/ }));
  expect(await screen.findByText(/copy failed/i)).toBeInTheDocument();
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

  fireEvent.click(screen.getByRole("button", { name: "set system proxy" }));
  expect(await screen.findByText(/use the manual steps below/)).toBeInTheDocument();
  expect(screen.getByTestId("set-proxy-status")).toHaveTextContent("couldn't set the proxy: denied");
  expect(screen.getByTestId("proxy-steps")).toHaveTextContent(/networksetup/);

  const status = screen.getByTestId("step-set-proxy-status");
  expect(status).toHaveTextContent(/couldn't set the proxy/);
  expect(status).not.toHaveTextContent(/system proxy is set/);
});

test("stop routing appears only after the proxy was set, and is wired to onUnsetProxy", async () => {
  const onUnsetProxy = vi.fn(async () => {});
  render(
    <Onboarding
      state={atSetProxy}
      dispatch={vi.fn()}
      os="macos"
      proxyAddr={ADDR}
      onSetProxy={vi.fn(async () => {})}
      onUnsetProxy={onUnsetProxy}
    />,
  );
  expect(screen.queryByRole("button", { name: "stop routing through Trawler" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "set system proxy" }));
  fireEvent.click(await screen.findByRole("button", { name: "stop routing through Trawler" }));
  expect(onUnsetProxy).toHaveBeenCalledOnce();
  expect(await screen.findByText(/set it again to resume/)).toBeInTheDocument();
});

// --- P1 prove-it + the honest per-step verdict ---

test("the last step renders the concrete prove-it command with the real address", () => {
  render(<Onboarding state={{ ...onboarding, step: "verify" }} dispatch={vi.fn()} proxyAddr={ADDR} />);
  expect(screen.getByText("curl -x http://10.1.2.3:9999 https://example.com")).toBeInTheDocument();
});

test("the trust status line reflects the probe: trusted → green, untrusted+traffic → device warning", () => {
  const { rerender } = render(
    <Onboarding state={onboarding} dispatch={vi.fn()} os="macos" trustProbe="trusted" onInstallCa={vi.fn()} />,
  );
  expect(screen.getByTestId("step-trust-ca-status")).toHaveTextContent(/trusted on this machine/);
  expect(screen.getByTestId("step-trust-ca")).toHaveAttribute("data-tone", "ok");

  rerender(
    <Onboarding
      state={onboarding}
      dispatch={vi.fn()}
      os="macos"
      trustProbe="untrusted"
      observed={{ decrypted: false, trafficSeen: true }}
      onInstallCa={vi.fn()}
    />,
  );
  expect(screen.getByTestId("step-trust-ca-status")).toHaveTextContent(/arriving but not decrypting/);
  expect(screen.getByTestId("step-trust-ca")).toHaveAttribute("data-tone", "warn");
});

// The verdict is a fragment; the instruction it used to carry is a Notice.
test("a long verdict keeps the fragment on the line and moves its remedy out", () => {
  render(
    <Onboarding
      state={atSetProxy}
      dispatch={vi.fn()}
      os="macos"
      proxyAddr={ADDR}
      proxyProbe="foreign"
      onSetProxy={vi.fn()}
    />,
  );

  const status = screen.getByTestId("step-set-proxy-status");
  expect(status).toHaveTextContent("another proxy is set");
  expect(status).not.toHaveTextContent(/point it at Trawler here/);
  expect(screen.getByTestId("set-proxy-remedy")).toHaveTextContent(/point it at Trawler here/);
});

test("the proxy status line goes green on a connection even with no decrypt", () => {
  render(
    <Onboarding
      state={atSetProxy}
      dispatch={vi.fn()}
      os="macos"
      proxyAddr={ADDR}
      proxyProbe="none"
      observed={{ decrypted: false, trafficSeen: true }}
      onSetProxy={vi.fn()}
    />,
  );
  expect(screen.getByTestId("step-set-proxy-status")).toHaveTextContent(/receiving traffic/);
  expect(screen.getByTestId("step-set-proxy")).toHaveAttribute("data-tone", "ok");
});

test("an un-probeable host shows 'can't check' rather than a false 'not set up'", () => {
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="linux" trustProbe="unknown" />);
  const status = screen.getByTestId("step-trust-ca-status");
  expect(status).toHaveTextContent(/can't check automatically/);
  expect(status).not.toHaveTextContent(/not trusted/);
});

test("back to flows shows only once ever proven", () => {
  const { rerender } = render(<Onboarding state={onboarding} dispatch={vi.fn()} />);
  expect(screen.queryByRole("button", { name: COPY.back })).toBeNull();
  rerender(<Onboarding state={{ ...onboarding, everProven: true }} dispatch={vi.fn()} />);
  expect(screen.getByRole("button", { name: COPY.back })).toBeInTheDocument();
});

test("no third tone map — the components own tone, the screen does not re-decide it", () => {
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="macos" trustProbe="trusted" />);
  const status = screen.getByTestId("step-trust-ca-status");
  expect(status.querySelector(".text-ok")).not.toBeNull();
});
