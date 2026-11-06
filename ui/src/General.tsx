import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Checkbox, Notice } from "./components/index";
import type { Invoke } from "./coreService";
import {
  LOGIN_OFF,
  readKeepCapturing,
  readLoginItem,
  readTrayAvailable,
  refusalWords,
  GNOME_TRAY_HINT,
  trayIcon,
  trayPlace,
  writeKeepCapturing,
  writeLoginItem,
  type KeepCapturing,
  type LoginItem,
} from "./generalPrefs";
import { hostOs, type HostOs } from "./setup";

interface Shown {
  keepCapturing: KeepCapturing;
  tray: boolean;
  login: LoginItem;
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
  const [keepCapturing, tray, login] = await Promise.all([
    readKeepCapturing(call).catch((): KeepCapturing => "off"),
    readTrayAvailable(call).catch(() => false),
    readLoginItem(call).catch(() => LOGIN_OFF),
  ]);
  return { keepCapturing, tray, login };
}

/**
 * Settings → General, as E-BGSVC.W5's design run drew it: the two switches the shell keeps (DEC-77), each writing
 * through its command, a refused write named under its box while the box shows what is stored.
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
    </div>
  );
}
