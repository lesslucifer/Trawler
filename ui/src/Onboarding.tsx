import { COPY, STEPS, stepStatus, type OnbEvent, type OnbState } from "./onboardingMachine";

// The onboarding surface: a centered stepper rendered from the state machine.
// Step actions are inert placeholders — the real CA install (W2), proxy set (W3)
// and live verify (W4) land later. Back to flows shows only once a list exists.

const GLYPH = { done: "✓ done", active: "▸ active", pending: "○ waiting" } as const;

export function Onboarding({
  state,
  dispatch,
}: {
  state: OnbState;
  dispatch: (e: OnbEvent) => void;
}) {
  const atFirst = state.step === STEPS[0];
  const atLast = state.step === STEPS[STEPS.length - 1];

  return (
    <div className="flex h-full flex-col overflow-auto">
      <div className="flex justify-end px-4 py-2">
        {state.everProven && (
          <button
            type="button"
            data-testid="onb-exit"
            onClick={() => dispatch({ t: "exit" })}
            className="rounded-button px-2 py-1 text-sm text-ink-mute hover:text-ink"
          >
            {COPY.back}
          </button>
        )}
      </div>

      <div className="flex flex-1 items-center justify-center p-6">
        <div className="w-full max-w-sm">
          <h2 className="text-center font-serif text-lg italic text-ink">{COPY.title}</h2>
          <p className="mb-4 text-center text-sm text-ink-mute">{COPY.subtitle}</p>

          <ol>
            {STEPS.map((step, i) => {
              const status = stepStatus(state, step);
              const copy = COPY.steps[step];
              return (
                <li
                  key={step}
                  data-testid={`step-${step}`}
                  data-status={status}
                  className={`my-2 rounded-lg border px-3 py-2 ${
                    status === "active" ? "border-ink-mute" : "border-hairline"
                  }`}
                >
                  <button
                    type="button"
                    onClick={() => dispatch({ t: "goto", step })}
                    className="flex w-full items-center justify-between text-left text-ink"
                  >
                    <span>
                      {i + 1} · {copy.title}
                    </span>
                    <small className="text-ink-mute">{GLYPH[status]}</small>
                  </button>

                  {status === "active" && (
                    <div data-testid="step-body" className="mt-2 text-sm text-ink-mute">
                      <p>{copy.body}</p>
                      {copy.action && (
                        <button
                          type="button"
                          disabled
                          title="Available in a later step of setup"
                          className="mt-2 rounded-button border border-hairline px-2 py-1 text-ink opacity-60"
                        >
                          {copy.action}
                        </button>
                      )}
                    </div>
                  )}
                </li>
              );
            })}
          </ol>

          <div className="mt-3 flex justify-between">
            <button
              type="button"
              data-testid="onb-back"
              disabled={atFirst}
              onClick={() => dispatch({ t: "back" })}
              className="rounded-button border border-hairline px-3 py-1 text-ink disabled:opacity-40"
            >
              Back
            </button>
            <button
              type="button"
              data-testid="onb-next"
              disabled={atLast}
              onClick={() => dispatch({ t: "next" })}
              className="rounded-button border border-hairline px-3 py-1 text-ink disabled:opacity-40"
            >
              Next
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
