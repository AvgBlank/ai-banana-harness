import { useCallback, useEffect, useState } from "react";
import { HomeScreen } from "@/components/home/HomeScreen";
import { SettingsView } from "@/components/settings/SettingsView";
import { ChangesPlaceholder } from "@/components/shell/ChangesPlaceholder";
import { IconRail } from "@/components/shell/IconRail";
import { Sidebar } from "@/components/shell/Sidebar";
import { TitleBar } from "@/components/shell/TitleBar";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useClient } from "@/state/client-context";
import { useSessions } from "@/state/sessions";
import { StateProvider, useApp } from "@/state/store";
import type { KeyStatus, Prefs } from "../shared/api";

const NARROW = "(max-width: 1279px)";

/** Below 1280 px the sidebar steps aside while Changes is open (the layout rule). */
function useNarrow(): boolean {
  const [narrow, setNarrow] = useState(() => window.matchMedia(NARROW).matches);
  useEffect(() => {
    const query = window.matchMedia(NARROW);
    const update = () => setNarrow(query.matches);
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  return narrow;
}

/** The clock for relative times; it ticks once a minute. */
function useNow(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

function Shell({ prefs }: { prefs: Prefs }) {
  const client = useClient();
  const { state, set } = useApp();
  const data = useSessions();
  const narrow = useNarrow();
  const now = useNow();
  const [appName, setAppName] = useState("");
  const [provider, setProvider] = useState<string | null>(null);
  const [key, setKey] = useState<KeyStatus | null>(null);
  const [recents, setRecents] = useState<string[]>(prefs.recents);

  useEffect(() => {
    client.appInfo().then(
      (info) => setAppName(info.name),
      () => undefined,
    );
  }, [client]);

  // Without a key nothing can be opened; with one, the workspace from last time comes back.
  useEffect(() => {
    let cancelled = false;
    client
      .keyStatus()
      .then(async (status) => {
        if (cancelled) return;
        setKey(status);
        if (status.present && prefs.lastWorkspace) {
          const opened = await client.openWorkspace(prefs.lastWorkspace).catch(() => null);
          if (opened && !cancelled) set({ workspace: opened.workspace });
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client, prefs.lastWorkspace, set]);

  // A key arriving later (check again, or typed in) brings the last workspace back too.
  const applyKey = useCallback(
    async (status: KeyStatus) => {
      setKey(status);
      if (status.present && state.workspace === null && prefs.lastWorkspace) {
        const opened = await client.openWorkspace(prefs.lastWorkspace).catch(() => null);
        if (opened) set({ workspace: opened.workspace });
      }
    },
    [client, prefs.lastWorkspace, set, state.workspace],
  );
  const recheckKey = useCallback(
    async () => applyKey(await client.recheckKey()),
    [client, applyKey],
  );
  const useKey = useCallback(
    async (typed: string) => applyKey(await client.setSessionKey(typed)),
    [client, applyKey],
  );

  // Picking a folder and choosing a recent one are the same flow: main opens it restricted.
  const openPath = useCallback(
    async (path: string) => {
      try {
        const opened = await client.openWorkspace(path);
        set({ workspace: opened.workspace, view: "home", sessionId: null });
        setRecents((before) =>
          [opened.workspace, ...before.filter((p) => p !== opened.workspace)].slice(0, 10),
        );
      } catch {
        // The folder could not be opened; the screen stays as it was.
      }
    },
    [client, set],
  );
  const openFolder = useCallback(async () => {
    const path = await client.pickFolder().catch(() => null);
    if (path) await openPath(path);
  }, [client, openPath]);

  const changesShown = state.changesOpen && state.view !== "settings";
  const sidebarShown = state.sidebarOpen && !(narrow && changesShown);
  const session = data.sessions.find((s) => s.id === state.sessionId);
  const recent = [
    ...new Set([
      ...(state.workspace ? [state.workspace] : []),
      ...recents,
      ...data.sessions.map((s) => s.workspace),
    ]),
  ];
  const canOpen = key?.present ?? false;

  return (
    <div className="flex h-full flex-col">
      <TitleBar sessionTitle={session ? (session.title ?? "Untitled session") : null} />
      <div className="flex min-h-0 flex-1">
        <IconRail />
        {sidebarShown && (
          <Sidebar
            appName={appName}
            data={data}
            now={now}
            canOpen={canOpen}
            onOpenFolder={openFolder}
          />
        )}
        <main className="min-w-0 flex-1 bg-bg">
          {state.view === "settings" ? (
            <SettingsView />
          ) : (
            <HomeScreen
              recent={recent}
              canOpen={canOpen}
              needsKey={key !== null && !key.present}
              appName={appName}
              onRecheckKey={recheckKey}
              onUseKey={useKey}
              provider={provider}
              onPickProvider={setProvider}
              onOpenFolder={openFolder}
              onOpenRecent={openPath}
            />
          )}
        </main>
        {changesShown && <ChangesPlaceholder />}
      </div>
    </div>
  );
}

export function App({ prefs }: { prefs: Prefs }) {
  return (
    <StateProvider prefs={prefs}>
      <TooltipProvider>
        <Shell prefs={prefs} />
      </TooltipProvider>
    </StateProvider>
  );
}
