import { useState } from "react";

export type StepMediaSource =
  | { kind: "image"; src: string }
  | { kind: "video"; src: { mp4: string; webm: string } };

const REDUCED_MOTION = "(prefers-reduced-motion: reduce)";

const prefersStill = () => window.matchMedia?.(REDUCED_MOTION)?.matches === true;

const FRAME = "block w-auto max-w-full max-h-step-media self-start rounded-card border border-hairline";

/**
 * StepMedia — the recording a by-hand step carries under its sentence, where a route opened a screen and the user must still find the control.
 * @tier T1 — props in, a figure out; no state but whether its file loaded
 * @anatomy the media at its own aspect, full width up to max-w-pane and never taller than step-media, at its own width rather than the column's, so a phone's portrait recording stands narrow, rounded-card with a hairline border · a caption under it naming the OS version it shows, chrome ink-mute · a video is muted, looped, inline, with native controls, and starts on mount except under prefers-reduced-motion, where it waits on its first frame
 * @use under a step whose route opened a screen, where the next control is a click or more away and its name alone does not find it
 * @avoid in place of the sentence — the step reads whole without it · where a route finishes the step · beside a command, whose block already is the step · a remote URL: the file ships in the app bundle, mp4 and webm both, so every webview plays one · decoration (motion-informs)
 * @states image · video looping with controls · video paused on its first frame under reduced motion · a file that does not load renders nothing, caption included
 * @madeof —
 */
export function StepMedia({ media, alt, shows }: { media: StepMediaSource; alt: string; shows: string }) {
  const [failed, setFailed] = useState(false);
  const [still] = useState(prefersStill);
  if (failed) return null;
  const fail = () => setFailed(true);
  return (
    <figure data-testid="step-media" className="m-0 flex max-w-pane flex-col gap-chrome-tight">
      {media.kind === "image" ? (
        <img src={media.src} alt={alt} onError={fail} className={FRAME} />
      ) : (
        <video aria-label={alt} autoPlay={!still} muted loop playsInline controls preload="metadata" className={FRAME}>
          <source src={media.src.mp4} type="video/mp4" />
          <source src={media.src.webm} type="video/webm" onError={fail} />
        </video>
      )}
      <figcaption className="font-ui text-chrome text-ink-mute">{shows}</figcaption>
    </figure>
  );
}
