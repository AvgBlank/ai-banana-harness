import { ArrowLeft, ArrowRight, FileDiff, PanelLeft } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/tooltip";
import { folderName } from "@/lib/format";
import { useApp } from "@/state/store";

export function TitleBar({
  sessionTitle,
  totals,
}: {
  sessionTitle: string | null;
  /** The current scope's line counts, shown on the Changes button once there are some. */
  totals: { added: number; removed: number } | null;
}) {
  const { state, set } = useApp();
  const workspace = state.workspace ? folderName(state.workspace) : null;

  return (
    <header
      className="drag relative flex shrink-0 items-center gap-1 border-b border-line bg-bg pr-3"
      // The left padding clears the traffic lights macOS draws over the hidden title bar.
      style={{ height: "var(--titlebar-h)", paddingLeft: 84 }}
    >
      <div className="no-drag flex items-center gap-0.5">
        <Tooltip label={state.sidebarOpen ? "Hide sidebar" : "Show sidebar"}>
          <Button
            variant="ghost"
            size="iconSm"
            aria-label="Toggle sidebar"
            aria-pressed={state.sidebarOpen}
            onClick={() => set({ sidebarOpen: !state.sidebarOpen })}
          >
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
        {sessionTitle ? (
          <>
            <span className="max-w-[40vw] truncate font-medium text-fg">{sessionTitle}</span>
            {workspace && (
              <>
                <span className="text-faint">in</span>
                <span className="text-muted">{workspace}</span>
              </>
            )}
          </>
        ) : (
          workspace && <span className="text-muted">{workspace}</span>
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
            {totals && totals.added + totals.removed > 0 && (
              <>
                <span className="font-mono text-[11px] text-add-fg">+{totals.added}</span>
                <span className="font-mono text-[11px] text-del-fg">−{totals.removed}</span>
              </>
            )}
          </Button>
        </Tooltip>
      </div>
    </header>
  );
}
