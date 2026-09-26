// What `cargo xtask ui walkthrough` reads before it runs anything: every reel,
// its launches in order, and each chapter's caption — the script without its acts.
import { REELS } from "./script.ts";

const plan = {
  reels: REELS.map((reel) => ({
    id: reel.id,
    host: reel.host,
    platform: reel.platform,
    launches: reel.launches.map((launch) => ({
      id: launch.id,
      chapters: launch.chapters.map(({ id, title, expect, findings }) => ({
        id,
        title,
        expect,
        findings,
      })),
    })),
  })),
};

process.stdout.write(`${JSON.stringify(plan)}\n`);
