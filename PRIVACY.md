# Privacy

Trawler keeps your traffic on your machine. It has no account, no analytics, no crash reporting and no telemetry.

| What | Where it goes |
|---|---|
| Requests you capture | Memory only, on your machine. Gone when you quit. Saved to a file only when you export HAR. |
| Trawler's certificate and its private key | Your app data folder, unencrypted: `~/Library/Application Support/dev.trawler.app/ca` (macOS) · `%APPDATA%\dev.trawler.app\ca` (Windows) · `~/.local/share/dev.trawler.app/ca` (Linux). Never sent anywhere. |
| Settings and window layout | Your app config folder. Never sent anywhere. |
| Update check | A request to GitHub when Trawler opens (at most once a day) and when you choose **Check for Updates…**. GitHub sees your IP address and Trawler's version. A switch in **Settings → General** turns the check at opening off. |
| Setup test | When you click the test on the setup screen, one request to `https://example.com`. |
| Other devices | Trawler listens only on this machine until you allow another device; then it also listens on the one network address you chose. |

Everything else Trawler sends is the traffic you route through it, going where it was already going.
