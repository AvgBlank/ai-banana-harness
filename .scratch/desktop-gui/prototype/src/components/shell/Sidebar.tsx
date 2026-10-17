import { AlertTriangle, ChevronDown, FolderOpen, FolderPlus, Pencil, Search, SquarePen } from "lucide-react";
import { useState } from "react";
import { RestrictedChip } from "@/components/banners/RestrictedChip";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/cn";
import { APP_NAME, SESSIONS, WORKSPACES } from "@/mock/data";
import { useApp } from "@/state/store";

export function Sidebar() {
  const { state, set } = useApp();
  const [titles, setTitles] = useState<Record<string, string>>({});
  const [renaming, setRenaming] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});

  return (
    <aside
      aria-label="Workspaces and sessions"
      className="flex shrink-0 flex-col border-r border-line bg-s1"
      style={{ width: "var(--sidebar-w)" }}
    >
      <div className="flex items-center justify-between px-4 pb-1 pt-3.5">
        <div className="flex items-center gap-1.5 text-[17px] font-semibold tracking-tight">
          {APP_NAME}
          <ChevronDown size={15} className="text-faint" />
        </div>
        <Tooltip label="Search sessions">
          <button aria-label="Search sessions" className="grid h-7 w-7 cursor-pointer place-items-center rounded-md text-muted hover:bg-s2 hover:text-fg">
            <Search size={16} />
          </button>
        </Tooltip>
      </div>

      <button
        onClick={() => set({ view: "home", sessionId: null })}
        className="mx-2 mt-2 flex h-9 cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-[14px] text-fg hover:bg-s2"
      >
        <SquarePen size={16} className="text-muted" />
        New session
      </button>

      <div className="mt-4 flex items-center justify-between px-4 text-[12px] font-medium text-muted">
        <span>Workspaces</span>
        <Tooltip label="Open folder…">
          <button aria-label="Open folder" className="grid h-6 w-6 cursor-pointer place-items-center rounded-md hover:bg-s2 hover:text-fg">
            <FolderPlus size={15} />
          </button>
        </Tooltip>
      </div>

      <div className="mt-1 min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        {WORKSPACES.map((ws) => {
          const sessions = SESSIONS.filter((s) => s.workspaceId === ws.id);
          const restricted = ws.hasConfig && !state.trustedWorkspaces.includes(ws.id);
          const open = !collapsed[ws.id];
          return (
            <div key={ws.id} className="mb-1">
              <button
                onClick={() => setCollapsed({ ...collapsed, [ws.id]: open })}
                className={cn(
                  "flex h-8 w-full cursor-pointer items-center gap-2 rounded-md px-2 text-left text-[14px] hover:bg-s2",
                  ws.id === state.workspaceId ? "text-fg" : "text-muted",
                )}
              >
                <FolderOpen size={16} className={ws.id === state.workspaceId ? "text-accent-text" : "text-faint"} />
                <span className="min-w-0 flex-1 truncate">{ws.name}</span>
                {restricted && <RestrictedChip compact />}
              </button>
              {open && (
                <ul className="ml-4 border-l border-line pl-2">
                  {sessions.length === 0 && <li className="px-2 py-1.5 text-[13px] text-faint">No sessions</li>}
                  {sessions.map((s) => {
                    const active = s.id === state.sessionId && state.view === "session";
                    return (
                      <li key={s.id} className="group relative">
                        {renaming === s.id ? (
                          <input
                            autoFocus
                            defaultValue={titles[s.id] ?? s.title}
                            aria-label="Rename session"
                            onBlur={(e) => {
                              setTitles({ ...titles, [s.id]: e.target.value.trim() || s.title });
                              setRenaming(null);
                            }}
                            onKeyDown={(e) => {
                              if (e.key === "Enter" || e.key === "Escape") (e.target as HTMLInputElement).blur();
                            }}
                            className="my-0.5 h-8 w-full rounded-md border border-accent-line bg-s2 px-2 text-[14px] text-fg outline-none"
                          />
                        ) : (
                          <button
                            onClick={() => set({ view: "session", sessionId: s.id, workspaceId: s.workspaceId })}
                            onDoubleClick={() => setRenaming(s.id)}
                            aria-current={active ? "true" : undefined}
                            className={cn(
                              "flex h-8 w-full cursor-pointer items-center gap-2 rounded-md px-2 text-left text-[14px] hover:bg-s2",
                              active ? "bg-s3 text-fg" : "text-muted hover:text-fg",
                            )}
                          >
                            <span className="min-w-0 flex-1 truncate">{titles[s.id] ?? s.title}</span>
                            {s.running && <span aria-label="Turn running" className="pulse-dot h-2 w-2 shrink-0 rounded-full bg-accent" />}
                            {s.interrupted && (
                              <Tooltip label="The last turn was interrupted when the sidecar stopped.">
                                <span aria-label="Interrupted"><AlertTriangle size={13} className="shrink-0 text-warn" /></span>
                              </Tooltip>
                            )}
                            <span className={cn("shrink-0 text-[12px] group-hover:invisible", active ? "text-muted" : "text-faint")}>{s.updated}</span>
                          </button>
                        )}
                        {renaming !== s.id && (
                          <Tooltip label="Rename session">
                            <button
                              aria-label={`Rename ${s.title}`}
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

      <div className="border-t border-line px-4 py-2.5 text-[12px] text-faint">Recent sessions · all workspaces</div>
    </aside>
  );
}
