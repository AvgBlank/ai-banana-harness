import {
  AlertTriangle,
  ChevronDown,
  FolderOpen,
  FolderPlus,
  Pencil,
  Search,
  ShieldAlert,
  SquarePen,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/cn";
import { folderName, relativeTime } from "@/lib/format";
import { groupByWorkspace, type SessionsData } from "@/state/sessions";
import { useApp } from "@/state/store";
import type { Session } from "../../../shared/protocol";

/** The inline rename field; it takes focus because the user just asked to rename. */
function RenameField({
  initial,
  onCommit,
}: {
  initial: string;
  onCommit: (value: string) => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);
  return (
    <input
      ref={input}
      defaultValue={initial}
      aria-label="Rename session"
      onBlur={(e) => onCommit(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === "Escape") e.currentTarget.blur();
      }}
      className="my-0.5 h-8 w-full rounded-md border border-accent-line bg-s2 px-2 text-[14px] text-fg outline-none"
    />
  );
}

export function Sidebar({
  appName,
  data,
  now,
  canOpen,
  restricted,
  onOpenFolder,
  onSelect,
}: {
  appName: string;
  data: SessionsData;
  now: number;
  canOpen: boolean;
  /** Workspaces with a config that is not trusted, so they run restricted. */
  restricted: ReadonlySet<string>;
  onOpenFolder: () => void;
  onSelect: (session: Session) => void;
}) {
  const { state, set } = useApp();
  const [renaming, setRenaming] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const groups = groupByWorkspace(data.sessions);

  const commitRename = (session: Session, value: string) => {
    setRenaming(null);
    const title = value.trim();
    if (title && title !== session.title) void data.rename(session, title).catch(() => undefined);
  };

  return (
    <aside
      aria-label="Workspaces and sessions"
      className="flex shrink-0 flex-col border-r border-line bg-s1"
      style={{ width: "var(--sidebar-w)" }}
    >
      <div className="flex items-center justify-between px-4 pb-1 pt-3.5">
        <div className="flex items-center gap-1.5 text-[17px] font-semibold tracking-tight">
          {appName}
          <ChevronDown size={15} className="text-faint" />
        </div>
        <Tooltip label="Search sessions">
          <button
            type="button"
            aria-label="Search sessions"
            disabled
            className="grid h-7 w-7 place-items-center rounded-md text-muted disabled:opacity-45"
          >
            <Search size={16} />
          </button>
        </Tooltip>
      </div>

      <button
        type="button"
        onClick={() => set({ view: "home", sessionId: null })}
        className="mx-2 mt-2 flex h-9 cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-[14px] text-fg hover:bg-s2"
      >
        <SquarePen size={16} className="text-muted" />
        New session
      </button>

      <div className="mt-4 flex items-center justify-between px-4 text-[12px] font-medium text-muted">
        <h2>Workspaces</h2>
        <Tooltip label="Open folder…">
          <button
            type="button"
            aria-label="Open folder"
            disabled={!canOpen}
            onClick={onOpenFolder}
            className="grid h-6 w-6 cursor-pointer place-items-center rounded-md hover:bg-s2 hover:text-fg disabled:cursor-not-allowed disabled:opacity-45"
          >
            <FolderPlus size={15} />
          </button>
        </Tooltip>
      </div>

      <div className="mt-1 min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        {data.status === "loading" && (
          <p className="px-2 py-1.5 text-[13px] text-faint">Starting…</p>
        )}
        {data.status === "ready" && groups.length === 0 && (
          <p className="px-2 py-1.5 text-[13px] text-faint">
            No sessions yet. Open a folder to begin.
          </p>
        )}
        {groups.map(({ workspace, sessions }) => {
          const open = !collapsed[workspace];
          const current = workspace === state.workspace;
          return (
            <div key={workspace} className="mb-1">
              <button
                type="button"
                aria-expanded={open}
                onClick={() => setCollapsed({ ...collapsed, [workspace]: open })}
                className={cn(
                  "flex h-8 w-full cursor-pointer items-center gap-2 rounded-md px-2 text-left text-[14px] hover:bg-s2",
                  current ? "text-fg" : "text-muted",
                )}
              >
                <FolderOpen size={16} className={current ? "text-accent-text" : "text-faint"} />
                <span className="min-w-0 flex-1 truncate" title={workspace}>
                  {folderName(workspace)}
                </span>
                {restricted.has(workspace) && (
                  <Tooltip label="Workspace config is not loaded">
                    <Badge tone="peel" className="gap-1">
                      <ShieldAlert size={11} />
                      Restricted
                    </Badge>
                  </Tooltip>
                )}
              </button>
              {open && (
                <ul className="ml-4 border-l border-line pl-2">
                  {sessions.map((s) => {
                    const active = s.id === state.sessionId && state.view === "session";
                    const title = s.title ?? "Untitled session";
                    return (
                      <li key={s.id} className="group relative">
                        {renaming === s.id ? (
                          <RenameField
                            initial={title}
                            onCommit={(value) => commitRename(s, value)}
                          />
                        ) : (
                          <button
                            type="button"
                            onClick={() => onSelect(s)}
                            onDoubleClick={() => setRenaming(s.id)}
                            aria-current={active ? "true" : undefined}
                            className={cn(
                              "flex h-8 w-full cursor-pointer items-center gap-2 rounded-md px-2 text-left text-[14px] hover:bg-s2",
                              active ? "bg-s3 text-fg" : "text-muted hover:text-fg",
                            )}
                          >
                            <span className="min-w-0 flex-1 truncate">{title}</span>
                            {data.running.has(s.id) && (
                              <span
                                role="img"
                                aria-label="Turn running"
                                className="pulse-dot h-2 w-2 shrink-0 rounded-full bg-accent"
                              />
                            )}
                            {data.interrupted.has(s.id) && (
                              <Tooltip label="The last turn was interrupted when the sidecar stopped.">
                                <span role="img" aria-label="Interrupted">
                                  <AlertTriangle size={13} className="shrink-0 text-warn" />
                                </span>
                              </Tooltip>
                            )}
                            <span
                              className={cn(
                                "shrink-0 text-[12px] group-hover:invisible",
                                active ? "text-muted" : "text-faint",
                              )}
                            >
                              {relativeTime(s.updated_at, now)}
                            </span>
                          </button>
                        )}
                        {renaming !== s.id && (
                          <Tooltip label="Rename session">
                            <button
                              type="button"
                              aria-label={`Rename ${title}`}
                              onClick={() => setRenaming(s.id)}
                              className="absolute right-1.5 top-1.5 hidden h-5 w-5 cursor-pointer place-items-center rounded text-muted hover:bg-s3 hover:text-fg group-hover:grid"
                            >
                              <Pencil size={12} />
                            </button>
                          </Tooltip>
                        )}
                      </li>
                    );
                  })}
                </ul>
              )}
            </div>
          );
        })}
      </div>

      <div className="border-t border-line px-4 py-2.5 text-[12px] text-faint">
        Recent sessions · all workspaces
      </div>
    </aside>
  );
}
