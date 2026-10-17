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
import { StateProvider, type Theme, useApp } from "@/state/store";

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

function Shell() {
  const client = useClient();
  const { state, set } = useApp();
  const data = useSessions();
  const narrow = useNarrow();
  const now = useNow();
  const [appName, setAppName] = useState("");
  const [provider, setProvider] = useState<string | null>(null);

  useEffect(() => {
    client.appInfo().then(
      (info) => setAppName(info.name),
      () => undefined,
    );
  }, [client]);

  const openFolder = useCallback(async () => {
    try {
      const path = await client.pickFolder();
      if (!path) return;
      const opened = await client.openWorkspace(path);
      set({ workspace: opened.workspace, view: "home", sessionId: null });
    } catch {
      // The folder could not be opened; the screen stays as it was.
    }
  }, [client, set]);

  const changesShown = state.changesOpen && state.view !== "settings";
  const sidebarShown = state.sidebarOpen && !(narrow && changesShown);
  const session = data.sessions.find((s) => s.id === state.sessionId);
  const recent = [
    ...new Set([
      ...(state.workspace ? [state.workspace] : []),
      ...data.sessions.map((s) => s.workspace),
    ]),
  ];

  return (
    <div className="flex h-full flex-col">
      <TitleBar sessionTitle={session ? (session.title ?? "Untitled session") : null} />
      <div className="flex min-h-0 flex-1">
        <IconRail />
        {sidebarShown && (
          <Sidebar appName={appName} data={data} now={now} onOpenFolder={openFolder} />
        )}
        <main className="min-w-0 flex-1 bg-bg">
          {state.view === "settings" ? (
            <SettingsView />
          ) : (
            <HomeScreen
              recent={recent}
              provider={provider}
              onPickProvider={setProvider}
              onOpenFolder={openFolder}
            />
          )}
        </main>
        {changesShown && <ChangesPlaceholder />}
      </div>
    </div>
  );
}

export function App({ theme }: { theme: Theme }) {
  return (
    <StateProvider theme={theme}>
      <TooltipProvider>
        <Shell />
      </TooltipProvider>
    </StateProvider>
  );
}
