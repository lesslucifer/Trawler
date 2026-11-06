import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Button, Checkbox, CopyText, Notice } from "./components/index";
import type { Invoke } from "./coreService";
import {
  checkForUpdates,
  LOGIN_OFF,
  readKeepCapturing,
  readLoginItem,
  readTrayAvailable,
  readUpdatePrefs,
  refusalWords,
  GNOME_TRAY_HINT,
  trayIcon,
  trayPlace,
  writeKeepCapturing,
  writeLoginItem,
  writeUpdateAutomatic,
  type KeepCapturing,
  type LoginItem,
  type UpdatePrefs,
} from "./generalPrefs";
import { hostOs, type HostOs } from "./setup";

interface Shown {
  keepCapturing: KeepCapturing;
  tray: boolean;
  login: LoginItem;
  updates: UpdatePrefs | null;
}

function Line({ children }: { children: string }) {
  return <p className="pt-chrome-tight font-ui text-chrome text-ink-mute">{children}</p>;
}

function Refusal({ why }: { why: string | null }) {
  if (why === null) return null;
  return (
    <div className="pt-chrome-tight">
      <Notice tone="warn">{`couldn't save — ${why}`}</Notice>
    </div>
  );
}

async function readAll(call: Invoke): Promise<Shown> {
  const [keepCapturing, tray, login, updates] = await Promise.all([
    readKeepCapturing(call).catch((): KeepCapturing => "off"),
    readTrayAvailable(call).catch(() => false),
    readLoginItem(call).catch(() => LOGIN_OFF),
    readUpdatePrefs(call).catch(() => null),
  ]);
  return { keepCapturing, tray, login, updates };
}

function Updates({ updates, call }: { updates: UpdatePrefs; call: Invoke }) {
  const [automatic, setAutomatic] = useState(updates.automatic);
  const [refused, setRefused] = useState<string | null>(null);

  const write = (on: boolean) => {
    setRefused(null);
    writeUpdateAutomatic(on, call).then(
      () => setAutomatic(on),
      async (error: unknown) => {
        setRefused(refusalWords(error));
        const stored = await readUpdatePrefs(call).catch(() => updates);
        setAutomatic(stored.automatic);
      },
    );
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-chrome-gap">
        <span className="font-ui text-body text-ink">Trawler</span>
        <CopyText value={updates.version} label="version" />
        {updates.installed && <Button onClick={() => void checkForUpdates(call)}>check now</Button>}
      </div>
      {updates.installed ? (
        <div>
          <Checkbox checked={automatic} onChange={write}>
            check for updates automatically
          </Checkbox>
          <Refusal why={refused} />
        </div>
      ) : (
        <div>
          <Checkbox checked={false} onChange={write} disabled>
            check for updates automatically
          </Checkbox>
          <div className="pt-chrome-tight">
            <Notice>updates come with an installed copy of Trawler</Notice>
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * Settings → General, as E-BGSVC.W5's design run drew it: the two switches the shell keeps (DEC-77), each writing
 * through its command, a refused write named under its box while the box shows what is stored; below them the
 * version, check now and the launch check's switch (E-PRORELEASE.W13's design run).
 */
export function General({ call = invoke, os = hostOs() }: { call?: Invoke; os?: HostOs }) {
  const [shown, setShown] = useState<Shown | null>(null);
  const [keepRefused, setKeepRefused] = useState<string | null>(null);
  const [loginRefused, setLoginRefused] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void readAll(call).then((read) => live && setShown(read));
    return () => {
      live = false;
    };
  }, [call]);

  if (shown === null) return null;

  const keep = (on: boolean) => {
    setKeepRefused(null);
    writeKeepCapturing(on, call).then(
      () => setShown((now) => now && { ...now, keepCapturing: on ? "on" : "off" }),
      async (error: unknown) => {
        setKeepRefused(refusalWords(error));
        const stored = await readKeepCapturing(call).catch((): KeepCapturing => "off");
        setShown((now) => now && { ...now, keepCapturing: stored });
      },
    );
  };

  const login = (on: boolean) => {
    setLoginRefused(null);
    writeLoginItem(on, call).then(
      (written) => setShown((now) => now && { ...now, login: written }),
      async (error: unknown) => {
        setLoginRefused(refusalWords(error));
        const stored = await readLoginItem(call).catch(() => LOGIN_OFF);
        setShown((now) => now && { ...now, login: stored });
      },
    );
  };

  return (
    <div className="flex flex-col gap-4">
      <h1 className="font-serif text-title italic">General</h1>
      <div>
        {shown.tray ? (
          <>
            <Checkbox checked={shown.keepCapturing === "on"} onChange={keep}>
              keep capturing when the window closes
            </Checkbox>
            <Line>{`Quit from the ${trayIcon(os)}.`}</Line>
          </>
        ) : (
          <>
            <Checkbox checked={false} onChange={keep} disabled>
              keep capturing when the window closes
            </Checkbox>
            <div className="pt-chrome-tight">
              <Notice>{`needs a ${trayIcon(os)}, which this desktop can't show${os === "linux" ? ` — ${GNOME_TRAY_HINT} one` : ""}`}</Notice>
            </div>
          </>
        )}
        <Refusal why={keepRefused} />
      </div>
      <div>
        <span title={shown.login.starts ?? undefined}>
          <Checkbox checked={shown.login.on} onChange={login}>
            open Trawler at login
          </Checkbox>
        </span>
        <Line>{shown.tray ? `Opens in ${trayPlace(os)}, with no window.` : "Opens with its window."}</Line>
        <Refusal why={loginRefused} />
      </div>
      {shown.updates && <Updates updates={shown.updates} call={call} />}
    </div>
  );
}
