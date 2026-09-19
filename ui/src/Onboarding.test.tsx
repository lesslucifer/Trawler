import { fireEvent, render, screen } from "@testing-library/react";
import { useReducer } from "react";
import { expect, test, vi } from "vitest";
import { Onboarding } from "./Onboarding";
import onboardingSource from "./Onboarding.tsx?raw";
import { COPY, reduce, type OnbState } from "./onboardingMachine";
import { guidedSteps, proxySnippet, proxySteps, SAVE_CA_STEP } from "./setup";
import { NO_TRAFFIC, RECENT_MS, TUNNEL_GRACE_MS } from "./verdicts";

vi.mock("./setup", async (importOriginal) => {
  const mod = await importOriginal<typeof import("./setup")>();
  return { ...mod, guidedSteps: vi.fn(mod.guidedSteps), proxySteps: vi.fn(mod.proxySteps) };
});

const onboarding: OnbState = { phase: "onboarding", step: "trust-ca", everProven: false };
const atSetProxy: OnbState = { phase: "onboarding", step: "set-proxy", everProven: false };
const atVerify: OnbState = { phase: "onboarding", step: "verify", everProven: false };
const ADDR = "10.1.2.3:9999";

// Traffic is judged against the screen's own clock, so a fixture states how long
// ago something arrived rather than when.
const ago = (ms: number) => Date.now() - ms;

function Harness({
  start = onboarding,
  ...props
}: { start?: OnbState } & Partial<
  Pick<
    Parameters<typeof Onboarding>[0],
    | "os"
    | "onInstallCa"
    | "onSaveCa"
    | "onRecheck"
    | "trustProbe"
    | "proxyProbe"
    | "probing"
    | "probedAt"
    | "traffic"
  >
>) {
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

test("the trust status line reflects the probe: trusted → green, untrusted → a calm not-yet", () => {
  const { rerender } = render(
    <Onboarding state={onboarding} dispatch={vi.fn()} os="macos" trustProbe="trusted" onInstallCa={vi.fn()} />,
  );
  expect(screen.getByTestId("step-trust-ca-status")).toHaveTextContent(/trusted on this machine/);
  expect(screen.getByTestId("step-trust-ca")).toHaveAttribute("data-tone", "ok");

  rerender(<Onboarding state={onboarding} dispatch={vi.fn()} os="macos" trustProbe="untrusted" onInstallCa={vi.fn()} />);
  expect(screen.getByTestId("step-trust-ca-status")).toHaveTextContent(/not trusted on this machine yet/);
  expect(screen.getByTestId("step-trust-ca")).toHaveAttribute("data-tone", "muted");
});

// The user's own finding: a flow captured earlier kept every step green after
// the proxy had been turned off. This machine and the traffic are two layers, and
// a step of one layer never borrows a verdict from the other.
test("traffic decrypting right now never turns a probed step green — it is step 3's verdict alone", () => {
  render(
    <Onboarding
      state={atVerify}
      dispatch={vi.fn()}
      os="macos"
      trustProbe="untrusted"
      proxyProbe="none"
      traffic={{ ...NO_TRAFFIC, lastDecryptedAt: ago(2_000) }}
    />,
  );

  expect(screen.getByTestId("step-verify-status")).toHaveTextContent("capturing traffic");
  expect(screen.getByTestId("step-verify")).toHaveAttribute("data-tone", "ok");
  expect(screen.getByTestId("step-trust-ca-status")).toHaveTextContent("not trusted on this machine yet");
  expect(screen.getByTestId("step-trust-ca")).not.toHaveAttribute("data-tone", "ok");
  expect(screen.getByTestId("step-set-proxy-status")).toHaveTextContent("not set on this machine yet");
  expect(screen.getByTestId("step-set-proxy")).not.toHaveAttribute("data-tone", "ok");
});

test("a flow older than the window reads idle, and the open step dates it", () => {
  const lastDecryptedAt = ago(RECENT_MS + 60_000);
  render(<Onboarding state={atVerify} dispatch={vi.fn()} traffic={{ ...NO_TRAFFIC, lastDecryptedAt }} />);

  const status = screen.getByTestId("step-verify-status");
  expect(status).toHaveTextContent("idle — nothing arriving now");
  expect(status).not.toHaveTextContent(/capturing/);
  expect(screen.getByTestId("step-verify")).toHaveAttribute("data-tone", "muted");
  expect(screen.getByTestId("last-flow")).toHaveTextContent(/last flow at \d\d:\d\d:\d\d/);
});

test("nothing ever seen shows no last-flow line — there is no flow to date", () => {
  render(<Onboarding state={atVerify} dispatch={vi.fn()} />);
  expect(screen.getByTestId("step-verify-status")).toHaveTextContent("waiting for your first request");
  expect(screen.queryByTestId("last-flow")).toBeNull();
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

test("a tunnel no flow answered warns on step 3 and says whose certificate is missing", () => {
  render(
    <Onboarding
      state={atVerify}
      dispatch={vi.fn()}
      trustProbe="trusted"
      traffic={{ ...NO_TRAFFIC, lastTunnelAt: ago(TUNNEL_GRACE_MS + 1_000) }}
    />,
  );

  const status = screen.getByTestId("step-verify-status");
  expect(status).toHaveTextContent("arriving but not decrypting");
  expect(status).not.toHaveTextContent(/doesn't trust Trawler's certificate/);
  expect(screen.getByTestId("verify-remedy")).toHaveTextContent(
    "the client sending it doesn't trust Trawler's certificate — install it there; step 1 covers this machine",
  );
  // The probe said trusted; the failing client is somewhere else, and step 1 keeps
  // saying what it knows rather than borrowing the failure.
  expect(screen.getByTestId("step-trust-ca-status")).toHaveTextContent("trusted on this machine");
});

test("an un-probeable trust step still says where the answer will show", () => {
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="linux" trustProbe="unknown" />);
  expect(screen.getByTestId("trust-ca-remedy")).toHaveTextContent("step 3 shows whether traffic decrypts");
});

// The failed install already states itself through the action's own Notice; a
// second copy of the same error under it would say it twice.
test("a failed install shows its error once, not twice", async () => {
  const onInstallCa = vi.fn(() => Promise.reject(new Error("declined")));
  render(<Harness start={onboarding} os="macos" onInstallCa={onInstallCa} />);

  fireEvent.click(screen.getByRole("button", { name: "install certificate" }));
  expect(await screen.findByText(/use the manual steps below/)).toBeInTheDocument();

  expect(screen.getByTestId("install-ca-status")).toHaveTextContent("declined");
  expect(screen.queryByTestId("trust-ca-remedy")).toBeNull();
});

test("the trust-CA command is copyable, not prose the user must retype", () => {
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="macos" onInstallCa={vi.fn()} />);

  expect(
    screen.getByRole("button", { name: /copy certificate command/ }),
  ).toBeInTheDocument();
  expect(screen.getByTestId("guided-steps")).toHaveTextContent(/security add-trusted-cert/);
});

test("the removal command is copyable too", () => {
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="macos" onInstallCa={vi.fn()} />);
  expect(screen.getByRole("button", { name: /copy removal command/ })).toBeInTheDocument();
});

test("a target whose steps are point-and-click offers no command to copy", () => {
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="macos" onInstallCa={vi.fn()} />);
  choose("trust target", "Firefox");

  expect(screen.queryByRole("button", { name: /copy certificate command/ })).toBeNull();
  expect(screen.getByTestId("guided-steps")).toHaveTextContent(/own certificate store/);
});

test("the proxy command appears exactly once — the step list is not doubled", () => {
  render(<Onboarding state={atSetProxy} dispatch={vi.fn()} os="macos" proxyAddr={ADDR} onSetProxy={vi.fn()} />);

  const snippet = proxySnippet("macos", ADDR)!;
  const steps = screen.getByTestId("proxy-steps");
  const shown = [...steps.querySelectorAll("li")].filter((li) => li.textContent?.includes(snippet));

  expect(shown).toHaveLength(1);
  expect(screen.getByRole("button", { name: /proxy command/ })).toBeInTheDocument();
});

test("the proxy-off command is copyable here too, not a bare string", () => {
  render(<Onboarding state={atSetProxy} dispatch={vi.fn()} os="macos" proxyAddr={ADDR} onSetProxy={vi.fn()} />);
  expect(screen.getByRole("button", { name: /proxy-off command/ })).toBeInTheDocument();
});

test("a failed set-proxy shows its error once, not twice", async () => {
  const onSetProxy = vi.fn(() => Promise.reject(new Error("denied")));
  const Harn = () => {
    const [state, dispatch] = useReducer(reduce, atSetProxy);
    return <Onboarding state={state} dispatch={dispatch} os="macos" proxyAddr={ADDR} onSetProxy={onSetProxy} />;
  };
  render(<Harn />);

  fireEvent.click(screen.getByRole("button", { name: "set system proxy" }));
  expect(await screen.findByText(/use the manual steps below/)).toBeInTheDocument();

  expect(screen.getByTestId("set-proxy-status")).toHaveTextContent("denied");
  expect(screen.queryByTestId("set-proxy-remedy")).toBeNull();
});

// The error lives in the action's own Notice, which only exists on the auto path.
// Switching the target away unmounts it, so the remedy must come back rather than
// leaving the error nowhere on screen.
test("switching target after a failed install keeps the error reachable", async () => {
  const onInstallCa = vi.fn(() => Promise.reject(new Error("elevation declined")));
  render(<Harness start={onboarding} os="macos" onInstallCa={onInstallCa} />);

  fireEvent.click(screen.getByRole("button", { name: "install certificate" }));
  expect(await screen.findByText(/use the manual steps below/)).toBeInTheDocument();

  choose("trust target", "Firefox");

  expect(screen.queryByTestId("install-ca-status")).toBeNull();
  expect(screen.getByTestId("trust-ca-remedy")).toHaveTextContent("elevation declined");
});

test("a connection never turns the proxy step green — the proxy step is this machine's probe", () => {
  render(
    <Onboarding
      state={atSetProxy}
      dispatch={vi.fn()}
      os="macos"
      proxyAddr={ADDR}
      proxyProbe="none"
      traffic={{ ...NO_TRAFFIC, lastTunnelAt: ago(TUNNEL_GRACE_MS + 1_000) }}
      onSetProxy={vi.fn()}
    />,
  );
  expect(screen.getByTestId("step-set-proxy-status")).toHaveTextContent("not set on this machine yet");
  expect(screen.getByTestId("step-set-proxy")).toHaveAttribute("data-tone", "muted");
  expect(screen.getByTestId("step-verify-status")).toHaveTextContent("arriving but not decrypting");
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

// A source-level invariant: no rendered assertion can catch a reintroduced tone
// map, because the screen's output would be indistinguishable from the
// component's. What is forbidden is mapping a VERDICT tone to a class — plain
// ink-mute body text is not that, so the check names the two decided tones.
test("no third tone map — the screen never maps a verdict tone to a class", () => {
  expect(onboardingSource).not.toMatch(/text-ok|text-warn/);
});

// "Save Trawler's certificate as trawler-ca.pem" named a file the screen never
// produced — every manual route began with an instruction the user could not
// carry out. The step that names the file is the step that produces it: the save
// control ends the sentence, and its outcome lands beneath, inside the same step.
test("the certificate is saved from the step that names it — a ↓ at the end of the sentence", async () => {
  const onSaveCa = vi.fn(async () => "trawler-ca.pem");
  render(<Harness start={onboarding} os="macos" onSaveCa={onSaveCa} />);

  const save = screen.getByRole("button", { name: "save certificate" });
  const step = save.closest("li")!;
  expect(screen.getByTestId("guided-steps")).toContainElement(step);
  expect(step).toHaveTextContent(SAVE_CA_STEP);
  expect(save).toHaveTextContent("↓");

  fireEvent.click(save);
  expect(onSaveCa).toHaveBeenCalledOnce();
  expect(await screen.findByText(/saved as trawler-ca\.pem/)).toBeInTheDocument();
  expect(step).toContainElement(screen.getByTestId("save-ca-status"));
});

test("without a save handler the step stays a sentence, with no dead control", () => {
  render(<Onboarding state={onboarding} dispatch={vi.fn()} os="linux" />);
  expect(screen.getByTestId("guided-steps")).toHaveTextContent(SAVE_CA_STEP);
  expect(screen.queryByRole("button", { name: "save certificate" })).toBeNull();
});

test("a failed save states why, and never claims the file landed", async () => {
  const onSaveCa = vi.fn(() => Promise.reject(new Error("GET /ca.crt failed: 500")));
  render(<Harness start={onboarding} os="macos" onSaveCa={onSaveCa} />);

  fireEvent.click(screen.getByRole("button", { name: "save certificate" }));
  expect(await screen.findByText(/couldn't save the certificate/)).toBeInTheDocument();
  expect(screen.getByTestId("save-ca-status")).toHaveTextContent("GET /ca.crt failed: 500");
  expect(screen.queryByText(/saved as trawler-ca\.pem/)).toBeNull();
});

// A probe is a reading taken at a moment. Nothing pushes when the user changes
// the system proxy in System Settings, so a green verdict outlives its truth
// until something asks again. One reading covers both probed steps, so one
// control re-takes it — above the steps, not inside each.
test("one check-again for this machine, above the steps, re-reads both probes", () => {
  const onRecheck = vi.fn();
  render(<Harness start={atVerify} os="macos" trustProbe="trusted" probedAt={Date.now()} onRecheck={onRecheck} />);

  const buttons = screen.getAllByRole("button", { name: "check again" });
  expect(buttons).toHaveLength(1);
  expect(screen.getByTestId("recheck")).toContainElement(buttons[0]);
  expect(screen.getByTestId("recheck")).toHaveTextContent(/checked at \d\d:\d\d:\d\d/);
  for (const step of ["step-trust-ca", "step-set-proxy", "step-verify"]) {
    expect(screen.getByTestId(step)).not.toContainElement(buttons[0]);
  }

  fireEvent.click(buttons[0]);
  expect(onRecheck).toHaveBeenCalledOnce();
});

// Fork D4: a control disabled in flight needs the progress line beside it. Here
// the line is the two probed verdicts themselves, in-flight — and only those two,
// since a probe says nothing about traffic.
test("while checking, the control is disabled and both probed steps show the in-flight verdict", () => {
  render(<Harness start={onboarding} os="macos" trustProbe="trusted" proxyProbe="ours" probing onRecheck={vi.fn()} />);

  expect(screen.getByRole("button", { name: "check again" })).toBeDisabled();
  expect(screen.getByTestId("recheck")).toHaveTextContent("checking this machine…");
  expect(screen.getByTestId("step-trust-ca-status")).toHaveTextContent("checking…");
  expect(screen.getByTestId("step-set-proxy-status")).toHaveTextContent("checking…");
  expect(screen.getByTestId("step-verify-status")).toHaveTextContent("waiting for your first request");
});

test("without a recheck handler the screen offers no control and states no time", () => {
  render(<Onboarding state={onboarding} dispatch={vi.fn()} probedAt={Date.now()} />);
  expect(screen.queryByTestId("recheck")).toBeNull();
});

test("re-checking reports a proxy that was green and is no longer", async () => {
  const view = render(
    <Onboarding
      state={atSetProxy}
      dispatch={vi.fn()}
      os="macos"
      proxyProbe="ours"
      onRecheck={vi.fn()}
    />,
  );
  expect(screen.getByTestId("step-set-proxy-status")).toHaveTextContent(/system proxy is set to Trawler/);

  view.rerender(
    <Onboarding
      state={atSetProxy}
      dispatch={vi.fn()}
      os="macos"
      proxyProbe="none"
      onRecheck={vi.fn()}
    />,
  );
  const status = screen.getByTestId("step-set-proxy-status");
  expect(status).toHaveTextContent("not set on this machine yet");
  expect(screen.getByTestId("step-set-proxy")).not.toHaveAttribute("data-tone", "ok");
});

// A cancelled save resolves exactly like a completed one. Reporting it as done
// tells the user a file exists, and step 2 then sends them to a terminal to run
// sudo against it.
test("a cancelled save says so, and never claims a file that was not written", async () => {
  const onSaveCa = vi.fn(async () => null);
  render(<Harness start={onboarding} os="macos" onSaveCa={onSaveCa} />);

  fireEvent.click(screen.getByRole("button", { name: "save certificate" }));
  expect(await screen.findByText(/nothing was saved/)).toBeInTheDocument();
  expect(screen.queryByText(/saved as trawler-ca\.pem/)).toBeNull();
});

// defaultPath only pre-fills the dialog's name input, so the user can save as
// anything. Naming the constant would state a file that isn't on disk, and the
// steps below run against that name.
test("a save under a different name reports the name that landed, not the one we asked for", async () => {
  const onSaveCa = vi.fn(async () => "/Users/someone/Downloads/ca-backup.pem");
  render(<Harness start={onboarding} os="macos" onSaveCa={onSaveCa} />);

  fireEvent.click(screen.getByRole("button", { name: "save certificate" }));
  expect(await screen.findByText(/saved as ca-backup\.pem/)).toBeInTheDocument();
  expect(screen.queryByText(/saved as trawler-ca\.pem/)).toBeNull();
});

// The native dialog resolves a full path; the steps below name a file, so the
// confirmation names the file and not the directory that holds it.
test("a save to the asked-for name under any directory confirms plainly", async () => {
  const onSaveCa = vi.fn(async () => "/Users/someone/Downloads/trawler-ca.pem");
  render(<Harness start={onboarding} os="macos" onSaveCa={onSaveCa} />);

  fireEvent.click(screen.getByRole("button", { name: "save certificate" }));
  const note = await screen.findByTestId("save-ca-status");
  expect(note).toHaveTextContent("saved as trawler-ca.pem. now trust it with the steps below.");
  expect(note).not.toHaveTextContent("/Users/someone/Downloads");
});
