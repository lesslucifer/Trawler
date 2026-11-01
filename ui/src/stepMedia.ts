import type { StepMediaSource } from "./components/index";
import type { MediaName } from "./setup";
import alwaysTrustMp4 from "./assets/setup/macos-always-trust.mp4";
import alwaysTrustWebm from "./assets/setup/macos-always-trust.webm";
import proxiesMp4 from "./assets/setup/macos-proxies.mp4";
import proxiesWebm from "./assets/setup/macos-proxies.webm";
import installCertMp4 from "./assets/setup/windows-install-cert.mp4";
import installCertWebm from "./assets/setup/windows-install-cert.webm";
import windowsProxyMp4 from "./assets/setup/windows-proxy.mp4";
import windowsProxyWebm from "./assets/setup/windows-proxy.webm";
import firefoxImportMp4 from "./assets/setup/linux-firefox-import.mp4";
import firefoxImportWebm from "./assets/setup/linux-firefox-import.webm";
import linuxProxyMp4 from "./assets/setup/linux-proxy.mp4";
import linuxProxyWebm from "./assets/setup/linux-proxy.webm";
import iosInstallProfileMp4 from "./assets/setup/ios-install-profile.mp4";
import iosInstallProfileWebm from "./assets/setup/ios-install-profile.webm";
import iosFullTrustMp4 from "./assets/setup/ios-full-trust.mp4";
import iosFullTrustWebm from "./assets/setup/ios-full-trust.webm";

// Each recording a step names, as the bundler fingerprints it: shipped with the
// app, never fetched (W12's M1).
export const STEP_MEDIA: Record<MediaName, StepMediaSource> = {
  "macos-always-trust": { kind: "video", src: { mp4: alwaysTrustMp4, webm: alwaysTrustWebm } },
  "macos-proxies": { kind: "video", src: { mp4: proxiesMp4, webm: proxiesWebm } },
  "windows-install-cert": { kind: "video", src: { mp4: installCertMp4, webm: installCertWebm } },
  "windows-proxy": { kind: "video", src: { mp4: windowsProxyMp4, webm: windowsProxyWebm } },
  "linux-firefox-import": { kind: "video", src: { mp4: firefoxImportMp4, webm: firefoxImportWebm } },
  "linux-proxy": { kind: "video", src: { mp4: linuxProxyMp4, webm: linuxProxyWebm } },
  "ios-install-profile": { kind: "video", src: { mp4: iosInstallProfileMp4, webm: iosInstallProfileWebm } },
  "ios-full-trust": { kind: "video", src: { mp4: iosFullTrustMp4, webm: iosFullTrustWebm } },
};
