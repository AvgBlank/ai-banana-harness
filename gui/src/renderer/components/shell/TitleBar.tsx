import { ArrowLeft, ArrowRight, Box, FileDiff, PanelLeft } from "lucide-react";
import { RestrictedChip } from "@/components/banners/RestrictedChip";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/tooltip";
import { folderName } from "@/lib/format";
import { useApp } from "@/state/store";
import type { CommandRunnerInfo } from "../../../shared/api";

export function TitleBar({
  sessionTitle,
  totals,
  restricted,
  runner,
  onReviewTrust,
}: {
  sessionTitle: string | null;
  /** The current scope's line counts, shown on the Changes button once there are some. */
  totals: { added: number; removed: number } | null;
  /** The workspace has a config that is not trusted, so it runs restricted. */
  restricted: boolean;
  /** How this workspace runs commands; only a container is worth a chip. */
  runner: CommandRunnerInfo | null;
  onReviewTrust: () => void;
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
        {restricted && <RestrictedChip onReview={onReviewTrust} />}
        {runner?.kind === "container" && (
          <Tooltip
            label={
              runner.problem
                ? `Commands are set to run in a container, but: ${runner.problem}`
                : `Commands run in a container (${runner.image}).`
            }
          >
            <span
              className={`flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11.5px] ${
                runner.problem ? "border-danger/50 text-danger" : "border-line text-muted"
              }`}
            >
              <Box size={12} />
              {runner.problem ? "Container unavailable" : "Container"}
            </span>
          </Tooltip>
        )}
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
