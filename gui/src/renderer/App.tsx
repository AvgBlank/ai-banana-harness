import { useCallback, useEffect, useState } from "react";
import { HomeScreen } from "@/components/home/HomeScreen";
import { SessionView } from "@/components/session/SessionView";
import { SettingsView } from "@/components/settings/SettingsView";
import { ChangesPlaceholder } from "@/components/shell/ChangesPlaceholder";
import { IconRail } from "@/components/shell/IconRail";
import { Sidebar } from "@/components/shell/Sidebar";
import { TitleBar } from "@/components/shell/TitleBar";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useClient } from "@/state/client-context";
import { useSessions } from "@/state/sessions";
import { StateProvider, useApp } from "@/state/store";
import { useTranscripts } from "@/state/transcripts";
import type { KeyStatus, Prefs } from "../shared/api";
import type { Session } from "../shared/protocol";

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
  const transcripts = useTranscripts(data.reload);
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

  const session = data.sessions.find((s) => s.id === state.sessionId);

  // A stored session needs its workspace open (restricted) before its turns can be read.
  const selectSession = useCallback(
    async (picked: Session) => {
      set({ view: "session", sessionId: picked.id, workspace: picked.workspace });
      const opened = await client.openWorkspace(picked.workspace).catch(() => null);
      if (opened) await transcripts.load(picked.workspace, picked.id);
      else transcripts.markFailed(picked.id);
    },
    [client, set, transcripts],
  );

  // From home: a new session in the open workspace, then its first turn.
  const startFromHome = useCallback(
    async (text: string) => {
      if (!state.workspace) throw new Error("Open a workspace first.");
      const created = await client.createSession(state.workspace, provider ?? undefined);
      data.add(created);
      set({ view: "session", sessionId: created.id });
      try {
        await transcripts.send(state.workspace, created.id, text);
      } catch (error) {
        // Back to home, where the text and the error are shown again.
        set({ view: "home", sessionId: null });
        throw error;
      }
      data.reload();
    },
    [client, data, provider, set, state.workspace, transcripts],
  );

  const sessionWorkspace = session?.workspace ?? state.workspace;
  const activeTurn = sessionWorkspace ? transcripts.state.active[sessionWorkspace] : undefined;
  const turnRunning = Boolean(
    activeTurn ||
      (session && data.running.has(session.id)) ||
      data.sessions.some((s) => s.workspace === sessionWorkspace && data.running.has(s.id)),
  );

  const changesShown = state.changesOpen && state.view !== "settings";
  const sidebarShown = state.sidebarOpen && !(narrow && changesShown);
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
            onSelect={selectSession}
          />
        )}
        <main className="min-w-0 flex-1 bg-bg">
          {state.view === "settings" ? (
            <SettingsView />
          ) : state.view === "session" && session ? (
            <SessionView
              turns={transcripts.state.turns[session.id] ?? []}
              load={transcripts.state.load[session.id]}
              running={turnRunning}
              stopping={activeTurn?.stopping ?? false}
              provider={provider}
              onPickProvider={setProvider}
              keyMissing={key !== null && !key.present}
              onSend={(text) => transcripts.send(session.workspace, session.id, text)}
              onStop={() => void transcripts.stop(session.workspace, session.id)}
            />
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
              onSend={startFromHome}
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
