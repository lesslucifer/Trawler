import type { StepMediaSource } from "./components/index";
import type { MediaName } from "./setup";
import alwaysTrustMp4 from "./assets/setup/macos-always-trust.mp4";
import alwaysTrustWebm from "./assets/setup/macos-always-trust.webm";
import proxiesMp4 from "./assets/setup/macos-proxies.mp4";
import proxiesWebm from "./assets/setup/macos-proxies.webm";

// Each recording a step names, as the bundler fingerprints it: shipped with the
// app, never fetched (W12's M1).
export const STEP_MEDIA: Record<MediaName, StepMediaSource> = {
  "macos-always-trust": { kind: "video", src: { mp4: alwaysTrustMp4, webm: alwaysTrustWebm } },
  "macos-proxies": { kind: "video", src: { mp4: proxiesMp4, webm: proxiesWebm } },
};
