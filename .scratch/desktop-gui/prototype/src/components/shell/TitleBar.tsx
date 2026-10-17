import { ArrowLeft, ArrowRight, FileDiff, PanelLeft } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/tooltip";
import { RestrictedChip } from "@/components/banners/RestrictedChip";
import { SESSIONS, TURN_CHANGES, WORKSPACES } from "@/mock/data";
import { useApp } from "@/state/store";

export function TitleBar() {
  const { state, set } = useApp();
  const ws = WORKSPACES.find((w) => w.id === state.workspaceId);
  const session = SESSIONS.find((s) => s.id === state.sessionId);
  const restricted = !!ws?.hasConfig && !state.trustedWorkspaces.includes(state.workspaceId);
  const showTitle = state.view === "session" && session;

  return (
    <header
      className="drag relative flex shrink-0 items-center gap-1 border-b border-line bg-bg px-3"
      style={{ height: "var(--titlebar-h)" }}
    >
      {/* Real traffic lights come from Electron's hiddenInset title bar; this is a stand-in. */}
      <div className="flex gap-2 pr-3" aria-hidden>
        <span className="h-3 w-3 rounded-full bg-[#ff5f57]" />
        <span className="h-3 w-3 rounded-full bg-[#febc2e]" />
        <span className="h-3 w-3 rounded-full bg-[#28c840]" />
      </div>
      <div className="no-drag flex items-center gap-0.5">
        <Tooltip label={state.sidebarOpen ? "Hide sidebar" : "Show sidebar"}>
          <Button variant="ghost" size="iconSm" aria-label="Toggle sidebar" onClick={() => set({ sidebarOpen: !state.sidebarOpen })}>
            <PanelLeft size={16} />
          </Button>
        </Tooltip>
        <Button variant="ghost" size="iconSm" aria-label="Back" disabled>
          <ArrowLeft size={16} />
        </Button>
        <Button variant="ghost" size="iconSm" aria-label="Forward" disabled>
          <ArrowRight size={16} />
        </Button>
      </div>

      <div className="pointer-events-none absolute inset-x-0 flex items-center justify-center gap-2 text-[13px]">
        {showTitle ? (
          <>
            <span className="max-w-[40vw] truncate font-medium text-fg">{session.title}</span>
            <span className="text-faint">in</span>
            <span className="text-muted">{ws?.name}</span>
          </>
        ) : (
          <span className="text-muted">{ws?.name}</span>
        )}
        {restricted && (
          <span className="pointer-events-auto no-drag">
            <RestrictedChip compact />
          </span>
        )}
      </div>

      <div className="no-drag ml-auto flex items-center gap-1">
        <Tooltip label={state.changesOpen ? "Hide changes" : "Show changes"}>
          <Button
            variant={state.changesOpen ? "secondary" : "ghost"}
            size="sm"
            aria-label="Toggle changes panel"
            aria-pressed={state.changesOpen}
            onClick={() => set({ changesOpen: !state.changesOpen })}
          >
            <FileDiff size={15} />
            <span className="font-mono text-[11px] text-add-fg">+{TURN_CHANGES.reduce((n, f) => n + f.added, 0)}</span>
            <span className="font-mono text-[11px] text-del-fg">−{TURN_CHANGES.reduce((n, f) => n + f.removed, 0)}</span>
          </Button>
        </Tooltip>
      </div>
    </header>
  );
}
