import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Button, CopyText, Notice, PageSidebar } from "./components/index";
import type { Invoke } from "./coreService";
import { ModalShell } from "./ModalShell";
import { PRIVACY_CLOSE, PRIVACY_LEAD, PRIVACY_ROWS } from "./privacy";

const PAGES = [
  { value: "about", label: "About" },
  { value: "licences", label: "Third-party licences" },
  { value: "privacy", label: "Privacy" },
] as const;

export type AboutPage = (typeof PAGES)[number]["value"];

/** What the shell asks About to show: a page, and the address of a link the browser would not open. */
export interface AboutShow {
  page: AboutPage;
  unopened: string | null;
}

interface AboutFacts {
  version: string;
  notice: string | null;
}

export type Link = "guide" | "issue" | "source";

const LINKS: readonly { link: Link; label: string }[] = [
  { link: "guide", label: "user guide" },
  { link: "issue", label: "report an issue" },
  { link: "source", label: "source code" },
];

function Caption({ children }: { children: string }) {
  return <p className="pt-chrome-tight font-ui text-chrome text-ink-mute">{children}</p>;
}

function AboutPageView({
  facts,
  unopened,
  open,
}: {
  facts: AboutFacts;
  unopened: string | null;
  open: (link: Link) => void;
}) {
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-3">
        <img src="/favicon.svg" alt="" aria-hidden="true" className="h-12 w-12" />
        <h1 className="font-serif text-title italic">Trawler</h1>
      </div>
      <div className="flex items-baseline gap-2">
        <span className="font-ui text-chrome text-ink-mute">version</span>
        <CopyText value={facts.version} label="version" />
      </div>
      <p className="font-ui text-body text-ink-body">MIT licence · © 2026 Vu Luu</p>
      <div className="flex flex-wrap gap-chrome-gap">
        {LINKS.map(({ link, label }) => (
          <Button key={link} onClick={() => open(link)}>
            {label}
          </Button>
        ))}
      </div>
      <Caption>
        Each opens in your browser. A report fills in your version and system; nothing is sent until you submit it.
      </Caption>
      {unopened !== null && (
        <Notice tone="warn" detail={<CopyText value={unopened} label="address" />}>
          couldn't open your browser — copy the address
        </Notice>
      )}
    </div>
  );
}

function LicencesPage({ notice }: { notice: string | null }) {
  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <h1 className="font-serif text-title italic">Third-party licences</h1>
      {notice === null ? (
        <Notice>the list comes with an installed copy of Trawler</Notice>
      ) : (
        <>
          <Caption>Every library and font inside this copy of Trawler, with its licence.</Caption>
          <pre
            tabIndex={0}
            role="region"
            aria-label="Third-party licences"
            className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap rounded-card border border-hairline p-3 font-mono text-value text-ink-body"
          >
            {notice}
          </pre>
        </>
      )}
    </div>
  );
}

function PrivacyPage() {
  return (
    <div className="flex flex-col gap-3">
      <h1 className="font-serif text-title italic">Privacy</h1>
      <p className="font-ui text-body text-ink">{PRIVACY_LEAD}</p>
      {PRIVACY_ROWS.map(({ what, where }) => (
        <div key={what}>
          <p className="font-ui text-body text-ink">{what}</p>
          <p className="font-ui text-body text-ink-body">{where}</p>
        </div>
      ))}
      <p className="font-ui text-body text-ink-body">{PRIVACY_CLOSE}</p>
    </div>
  );
}

/** About (E-PRORELEASE.W13's design run): About · Third-party licences · Privacy, on every OS (DEC-90). */
export function About({ call = invoke, show }: { call?: Invoke; show: AboutShow }) {
  const [shown, setShown] = useState<AboutShow>(show);
  const [facts, setFacts] = useState<AboutFacts | null>(null);

  useEffect(() => {
    let live = true;
    void call<AboutFacts>("about_read").then((read) => live && setFacts(read));
    return () => {
      live = false;
    };
  }, [call]);

  useEffect(() => setShown(show), [show]);

  const open = (link: Link) => {
    setShown((now) => ({ ...now, unopened: null }));
    call<void>("open_link", { link }).catch((address: unknown) =>
      setShown((now) => ({ ...now, unopened: String(address) })),
    );
  };

  return (
    <div className="flex min-h-0 flex-1 gap-4 font-ui text-body text-ink">
      <PageSidebar
        pages={PAGES}
        value={shown.page}
        onChange={(next) => setShown((now) => ({ ...now, page: next as AboutPage }))}
        ariaLabel="About pages"
      />
      <div
        role="tabpanel"
        aria-label={PAGES.find((p) => p.value === shown.page)?.label}
        className="min-h-0 min-w-0 flex-1 overflow-auto"
      >
        {facts === null ? null : shown.page === "about" ? (
          <AboutPageView facts={facts} unopened={shown.unopened} open={open} />
        ) : shown.page === "licences" ? (
          <LicencesPage notice={facts.notice} />
        ) : (
          <PrivacyPage />
        )}
      </div>
    </div>
  );
}

/** About as the main window's dialog (W19), opened on the page the shell asked for. */
export function AboutDialog({ show, onClose, call }: { show: AboutShow; onClose: () => void; call?: Invoke }) {
  return (
    <ModalShell title="About Trawler" testId="about" size="pages" onCancel={onClose} dismissLabel="close">
      <About call={call} show={show} />
    </ModalShell>
  );
}
