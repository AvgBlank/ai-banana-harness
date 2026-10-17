import { useCallback, useEffect, useRef, useState } from "react";
import { CrashBanner } from "@/components/banners/CrashBanner";
import { ChangesPanel, pickFile, totals } from "@/components/changes/ChangesPanel";
import { ApprovalDialog } from "@/components/dialogs/ApprovalDialog";
import { QuestionDialog } from "@/components/dialogs/QuestionDialog";
import { type ReviewState, TrustDialog } from "@/components/dialogs/TrustDialog";
import { UndoDialog } from "@/components/dialogs/UndoDialog";
import { HomeScreen } from "@/components/home/HomeScreen";
import { SessionView } from "@/components/session/SessionView";
import { SettingsView } from "@/components/settings/SettingsView";
import { IconRail } from "@/components/shell/IconRail";
import { Sidebar } from "@/components/shell/Sidebar";
import { TitleBar } from "@/components/shell/TitleBar";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useChanges } from "@/state/changes";
import { useClient } from "@/state/client-context";
import { useSessions } from "@/state/sessions";
import { StateProvider, useApp } from "@/state/store";
import { pendingOf, useTranscripts } from "@/state/transcripts";
import type { KeyStatus, OpenedWorkspace, Prefs, SidecarState, TrustState } from "../shared/api";
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

const messageOf = (error: unknown, fallback: string): string =>
  typeof error === "object" && error !== null && "message" in error
    ? String(error.message)
    : fallback;

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
  const reloadSessions = data.reload;
  const changesRef = useRef<ReturnType<typeof useChanges> | null>(null);
  // Sessions refresh at turn edges; changes refresh when a turn ends or a file tool answers.
  const transcripts = useTranscripts(
    useCallback(
      (info) => {
        if (info.type === "turn.created" || info.type === "turn.done") reloadSessions();
        if (info.type === "turn.done") changesRef.current?.refresh("both", { immediate: true });
        else if (info.mutating) changesRef.current?.refresh("both");
      },
      [reloadSessions],
    ),
  );
  const narrow = useNarrow();
  const now = useNow();
  const [appName, setAppName] = useState("");
  const [provider, setProvider] = useState<string | null>(null);
  const [key, setKey] = useState<KeyStatus | null>(null);
  const [sidecar, setSidecar] = useState<SidecarState>({ state: "starting" });
  const [trust, setTrust] = useState<Record<string, TrustState>>({});
  const trustRef = useRef(trust);
  trustRef.current = trust;
  const [notice, setNotice] = useState<string | null>(null);
  const [review, setReview] = useState<{
    workspace: string;
    state: ReviewState;
    busy: boolean;
    error: string | null;
  } | null>(null);
  const [recents, setRecents] = useState<string[]>(prefs.recents);

  const track = useCallback((opened: OpenedWorkspace) => {
    setTrust((before) => ({ ...before, [opened.workspace]: opened.trust }));
  }, []);
  // A workspace main refused to open (its config would move the API key) is told, not hidden.
  const reportRefusal = useCallback((error: unknown) => {
    if (
      typeof error === "object" &&
      error !== null &&
      "code" in error &&
      error.code === "blocked_config"
    ) {
      setNotice(messageOf(error, "Blocked."));
    }
  }, []);

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
          const opened = await client.openWorkspace(prefs.lastWorkspace).catch((e) => {
            reportRefusal(e);
            return null;
          });
          if (opened && !cancelled) {
            track(opened);
            set({ workspace: opened.workspace });
          }
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client, prefs.lastWorkspace, set, track, reportRefusal]);

  // A key arriving later (check again, or typed in) brings the last workspace back too.
  const applyKey = useCallback(
    async (status: KeyStatus) => {
      setKey(status);
      if (status.present && state.workspace === null && prefs.lastWorkspace) {
        const opened = await client.openWorkspace(prefs.lastWorkspace).catch((e) => {
          reportRefusal(e);
          return null;
        });
        if (opened) {
          track(opened);
          set({ workspace: opened.workspace });
        }
      }
    },
    [client, prefs.lastWorkspace, set, state.workspace, track, reportRefusal],
  );
  const recheckKey = useCallback(
    async () => applyKey(await client.recheckKey()),
    [client, applyKey],
  );
  const useKey = useCallback(
    async (typed: string) => applyKey(await client.setSessionKey(typed)),
    [client, applyKey],
  );

  const startReview = useCallback(
    async (workspace: string) => {
      setReview({ workspace, state: { status: "loading" }, busy: false, error: null });
      try {
        const loaded = await client.reviewTrust(workspace);
        setReview({
          workspace,
          state: { status: "ready", review: loaded },
          busy: false,
          error: null,
        });
      } catch (error) {
        const message = messageOf(error, "The config could not be read.");
        setReview({ workspace, state: { status: "error", message }, busy: false, error: null });
      }
    },
    [client],
  );

  const grantTrust = useCallback(
    async (acknowledgeHosts: boolean) => {
      if (review?.state.status !== "ready") return;
      const { workspace } = review;
      setReview({ ...review, busy: true, error: null });
      try {
        track(
          await client.trustWorkspace({
            path: workspace,
            sha256: review.state.review.sha256,
            acknowledgeHosts,
          }),
        );
        setReview(null);
      } catch (error) {
        const blocked =
          typeof error === "object" &&
          error !== null &&
          "code" in error &&
          error.code === "blocked_config";
        if (blocked) {
          // The sidecar closed the workspace: it must not stay on screen.
          setNotice(messageOf(error, "Blocked."));
          setTrust((before) => ({ ...before, [workspace]: "needs_review" }));
          if (state.workspace === workspace)
            set({ workspace: null, view: "home", sessionId: null });
          setReview(null);
        } else {
          setReview({ ...review, busy: false, error: messageOf(error, "Could not trust it.") });
        }
      }
    },
    [client, review, set, state.workspace, track],
  );

  // Picking a folder and choosing a recent one are the same flow: main opens it restricted.
  const openPath = useCallback(
    async (path: string) => {
      try {
        const opened = await client.openWorkspace(path);
        track(opened);
        set({ workspace: opened.workspace, view: "home", sessionId: null });
        if (opened.trust === "needs_review") void startReview(opened.workspace);
        setRecents((before) =>
          [opened.workspace, ...before.filter((p) => p !== opened.workspace)].slice(0, 10),
        );
      } catch (error) {
        // The folder could not be opened; the screen stays as it was.
        reportRefusal(error);
      }
    },
    [client, set, track, reportRefusal, startReview],
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
      const opened = await client.openWorkspace(picked.workspace).catch((e) => {
        reportRefusal(e);
        return null;
      });
      if (opened) {
        track(opened);
        await transcripts.load(picked.workspace, picked.id);
      } else transcripts.markFailed(picked.id);
    },
    [client, set, transcripts, track, reportRefusal],
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

  // Required actions of the session on screen, oldest first; approvals and questions queue together.
  const pending = session ? pendingOf(transcripts.state, session.id) : [];
  const nextAction = pending[0];
  const approvals = pending.filter((a) => a.kind === "approval");

  // The sidecar's state drives recovery. A crash interrupts what was running; a return resumes the
  // visible session from the database and attaches to running turns. A reloaded window asks for
  // the current state once, so it does not wait for a push that already happened.
  const visibleRef = useRef(session);
  visibleRef.current = session;
  const { reconcile, load, catchUp, interruptAll } = transcripts;
  useEffect(() => {
    const onState = (status: SidecarState) => {
      setSidecar(status);
      if (status.state === "crashed" || status.state === "failed") {
        interruptAll();
        return;
      }
      if (status.state !== "ready") return;
      void reconcile();
      // A restarted sidecar opens everything restricted; each workspace is opened anew so trust is
      // judged again against the file as it is now.
      for (const workspace of Object.keys(trustRef.current)) {
        client.openWorkspace(workspace).then(track, reportRefusal);
      }
      const visible = visibleRef.current;
      if (visible) void load(visible.workspace, visible.id);
      void catchUp();
    };
    client.sidecarState().then(onState, () => undefined);
    return client.onSidecarState(onState);
  }, [client, reconcile, load, catchUp, interruptAll, track, reportRefusal]);

  const switchProvider = useCallback(
    async (id: string) => {
      if (state.view !== "session" || !session) {
        setProvider(id);
        return;
      }
      try {
        data.add(await client.switchProvider(session.workspace, session.id, id));
        setProvider(id);
      } catch {
        // The sidecar refused (for example, a turn is running); the session keeps its provider.
      }
    },
    [client, data, session, state.view],
  );

  const [undo, setUndo] = useState<{
    confirming: boolean;
    busy: boolean;
    error: string | null;
    hidden: boolean;
  }>({ confirming: false, busy: false, error: null, hidden: false });
  const [hints, setHints] = useState<Record<string, string>>({});

  const changes = useChanges(
    session ? { workspace: session.workspace, sessionId: session.id } : null,
  );
  changesRef.current = changes;

  const sessionWorkspace = session?.workspace ?? state.workspace;
  const activeTurn = sessionWorkspace ? transcripts.state.active[sessionWorkspace] : undefined;
  const turnRunning = Boolean(
    activeTurn ||
      (session && data.running.has(session.id)) ||
      data.sessions.some((s) => s.workspace === sessionWorkspace && data.running.has(s.id)),
  );

  const currentWorkspace = session?.workspace ?? state.workspace;
  const currentTrust = currentWorkspace ? trust[currentWorkspace] : undefined;
  const restricted = currentTrust === "needs_review" || currentTrust === "unreadable";
  const restrictedWorkspaces = new Set(
    Object.entries(trust).flatMap(([workspace, value]) =>
      value === "needs_review" || value === "unreadable" ? [workspace] : [],
    ),
  );

  const changesShown = state.changesOpen && state.view !== "settings";
  const scopeEntry = changes[state.changesScope];
  const lastTurn = session ? transcripts.state.turns[session.id]?.at(-1) : undefined;
  const lastTurnFinished = lastTurn !== undefined && lastTurn.status !== "running";
  const canUndo =
    lastTurn !== undefined &&
    lastTurnFinished &&
    lastTurn.turnId !== null &&
    !lastTurn.undone &&
    !undo.hidden &&
    changes.turn.checkpoint === true &&
    !turnRunning;

  const { refresh } = changes;
  // Opening the panel, switching scope, changing session and returning to the window all ask again.
  // biome-ignore lint/correctness/useExhaustiveDependencies: these are the triggers
  useEffect(() => {
    if (session && state.view === "session") refresh("both", { immediate: true });
  }, [session?.id, state.view, state.changesScope, changesShown, refresh]);
  useEffect(() => {
    const onFocus = () => refresh("both");
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refresh]);
  // A new last turn starts with a fresh undo state.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the turn id is the trigger
  useEffect(() => {
    setUndo({ confirming: false, busy: false, error: null, hidden: false });
  }, [session?.id, lastTurn?.turnId]);

  const confirmUndo = useCallback(async () => {
    if (!session || !lastTurn?.turnId) return;
    setUndo((u) => ({ ...u, busy: true, error: null }));
    try {
      const result = await client.undo(session.workspace, session.id);
      if (result.outcome === "restored" && result.event) {
        transcripts.applyEvent(session.workspace, session.id, lastTurn.turnId, result.event);
        setHints((h) => ({
          ...h,
          [session.id]:
            "The files were restored, but the agent still remembers the turn. Say so in your next message.",
        }));
        setUndo({ confirming: false, busy: false, error: null, hidden: false });
      } else {
        setUndo({ confirming: false, busy: false, error: null, hidden: true });
      }
      refresh("both", { immediate: true });
    } catch (error) {
      setUndo({
        confirming: false,
        busy: false,
        error: error instanceof Error ? error.message : "The undo failed.",
        hidden: false,
      });
    }
  }, [client, lastTurn?.turnId, refresh, session, transcripts]);
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
      <TitleBar
        sessionTitle={session ? (session.title ?? "Untitled session") : null}
        totals={changesShown || session ? totals(scopeEntry.files) : null}
        restricted={restricted}
        onReviewTrust={() => currentWorkspace && void startReview(currentWorkspace)}
      />
      <CrashBanner
        state={sidecar}
        appName={appName}
        onRestart={() => void client.restartSidecar().catch(() => undefined)}
      />
      {notice && (
        <div
          role="alert"
          className="flex items-center gap-3 border-b border-danger/40 bg-danger-soft px-4 py-2 text-[13px] text-danger"
        >
          <span className="min-w-0 flex-1">{notice}</span>
          <button
            type="button"
            onClick={() => setNotice(null)}
            className="cursor-pointer text-[12.5px] underline"
          >
            Dismiss
          </button>
        </div>
      )}
      <div className="flex min-h-0 flex-1">
        <IconRail />
        {sidebarShown && (
          <Sidebar
            appName={appName}
            data={data}
            now={now}
            canOpen={canOpen}
            restricted={restrictedWorkspaces}
            onOpenFolder={openFolder}
            onSelect={selectSession}
          />
        )}
        <main className="min-w-0 flex-1 bg-bg">
          {state.view === "settings" ? (
            <SettingsView
              keyStatus={key}
              onRecheckKey={recheckKey}
              onForgotten={(opened) => opened && track(opened)}
            />
          ) : state.view === "session" && session ? (
            <SessionView
              turns={transcripts.state.turns[session.id] ?? []}
              load={transcripts.state.load[session.id]}
              running={turnRunning}
              stopping={activeTurn?.stopping ?? false}
              provider={session.provider}
              onPickProvider={switchProvider}
              keyMissing={key !== null && !key.present}
              sidecarDown={sidecar.state === "crashed" || sidecar.state === "failed"}
              onSend={(text) => {
                setHints(({ [session.id]: _gone, ...rest }) => rest);
                return transcripts.send(session.workspace, session.id, text);
              }}
              hint={hints[session.id] ?? null}
              undo={{
                canUndo,
                undoing: undo.busy,
                error: undo.error,
                onUndo: () => setUndo((u) => ({ ...u, confirming: true, error: null })),
              }}
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
        {changesShown && (
          <ChangesPanel
            scope={state.changesScope}
            entry={scopeEntry}
            selectedPath={
              pickFile(scopeEntry.files, state.selectedFile[state.changesScope])?.path ?? null
            }
            lastTurnFinished={lastTurnFinished}
            width={state.changesWidth}
            onScope={(changesScope) => set({ changesScope })}
            onSelect={(path) =>
              set({ selectedFile: { ...state.selectedFile, [state.changesScope]: path } })
            }
            onResize={(changesWidth) => set({ changesWidth })}
            onRetry={() => refresh(state.changesScope, { immediate: true })}
            onClose={() => set({ changesOpen: false })}
          />
        )}
      </div>
      <TrustDialog
        open={review !== null}
        state={review?.state ?? { status: "loading" }}
        busy={review?.busy ?? false}
        error={review?.error ?? null}
        onRestricted={() => setReview(null)}
        onTrust={(acknowledge) => void grantTrust(acknowledge)}
      />
      {session && (
        <>
          <ApprovalDialog
            action={nextAction?.kind === "approval" ? nextAction : undefined}
            position={nextAction?.kind === "approval" ? approvals.indexOf(nextAction) + 1 : 0}
            total={approvals.length}
            workspace={session.workspace}
            onAnswer={(action, answer) =>
              void transcripts.reply(session.workspace, session.id, action.actionId, answer)
            }
          />
          <UndoDialog
            open={undo.confirming}
            files={changes.turn.files}
            busy={undo.busy}
            onKeep={() => setUndo((u) => ({ ...u, confirming: false }))}
            onConfirm={() => void confirmUndo()}
          />
          <QuestionDialog
            action={nextAction?.kind === "question" ? nextAction : undefined}
            onAnswer={(action, text) =>
              void transcripts.reply(session.workspace, session.id, action.actionId, { text })
            }
          />
        </>
      )}
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
