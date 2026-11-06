// The one privacy statement: the About window's Privacy page renders it and
// PRIVACY.md (E-PRORELEASE.W5) carries the same words. Draft voice: W11.
export const PRIVACY_LEAD =
  "Trawler keeps your traffic on your machine. It has no account, no analytics, no crash reporting and no telemetry.";

export interface PrivacyRow {
  what: string;
  where: string;
}

export const PRIVACY_ROWS: readonly PrivacyRow[] = [
  {
    what: "Requests you capture",
    where: "Memory only, on your machine. Gone when you quit. Saved to a file only when you export HAR.",
  },
  {
    what: "Trawler's certificate and its private key",
    where:
      "Your app data folder, unencrypted: ~/Library/Application Support/dev.trawler.app/ca (macOS) · %APPDATA%\\dev.trawler.app\\ca (Windows) · ~/.local/share/dev.trawler.app/ca (Linux). Never sent anywhere.",
  },
  { what: "Settings and window layout", where: "Your app config folder. Never sent anywhere." },
  {
    what: "Update check",
    where:
      "A request to GitHub when Trawler opens (at most once a day) and when you choose Check for Updates…. GitHub sees your IP address and Trawler's version. Settings → General → check for updates automatically turns the check when Trawler opens off.",
  },
  {
    what: "Setup test",
    where: "When you click the test on the setup screen, one request to https://example.com.",
  },
  {
    what: "Other devices",
    where:
      "Trawler listens only on this machine until you allow another device; then it also listens on the one network address you chose.",
  },
];

export const PRIVACY_CLOSE =
  "Everything else Trawler sends is the traffic you route through it, going where it was already going.";
