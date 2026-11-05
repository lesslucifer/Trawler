import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { CaptureService } from "./CaptureService";
import { PageSidebar } from "./components/index";
import type { Invoke } from "./coreService";
import { General } from "./General";
import { tauriBridge, type CoreBridge } from "./useCoreService";

const PAGES = [
  { value: "general", label: "General" },
  { value: "service", label: "Capture service" },
] as const;

type SettingsPage = (typeof PAGES)[number]["value"];

/** The Settings window: its pages in a sidebar (E-BGSVC.W5's design run), General first. */
export function Settings({ call = invoke, bridge = tauriBridge }: { call?: Invoke; bridge?: CoreBridge }) {
  const [page, setPage] = useState<SettingsPage>("general");
  return (
    <div className="flex h-full gap-4 bg-canvas p-4 font-ui text-body text-ink">
      <PageSidebar
        pages={PAGES}
        value={page}
        onChange={(next) => setPage(next as SettingsPage)}
        ariaLabel="Settings pages"
      />
      <div role="tabpanel" aria-label={PAGES.find((p) => p.value === page)?.label} className="min-w-0 flex-1">
        {page === "general" ? <General call={call} /> : <CaptureService bridge={bridge} />}
      </div>
    </div>
  );
}
