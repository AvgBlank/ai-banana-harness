import { Check, ChevronDown, Folder, FolderPlus } from "lucide-react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/cn";
import { folderName } from "@/lib/format";
import { useApp } from "@/state/store";

export function WorkspacePicker({
  recent,
  canOpen,
  onOpenFolder,
  onOpenRecent,
}: {
  recent: string[];
  /** False while there is no API key: no workspace can be opened. */
  canOpen: boolean;
  onOpenFolder: () => void;
  onOpenRecent: (path: string) => Promise<void>;
}) {
  const { state, set } = useApp();
  const current = state.workspace;
  return (
    <div className="flex items-center gap-2 rounded-t-[18px] border border-b-0 border-line bg-s1 px-3.5 py-2">
      <Popover
        open={state.workspaceMenuOpen}
        onOpenChange={(workspaceMenuOpen) => set({ workspaceMenuOpen })}
      >
        <PopoverTrigger asChild>
          <button
            type="button"
            aria-label={`Workspace: ${current ? folderName(current) : "none chosen"}`}
            className="flex h-7 cursor-pointer items-center gap-2 rounded-md pr-1.5 text-[14px] text-fg hover:bg-s2"
          >
            <Folder size={16} className="text-accent-text" />
            {current ? folderName(current) : "Choose a workspace"}
            <ChevronDown size={13} className="text-faint" />
          </button>
        </PopoverTrigger>
        <PopoverContent side="top" className="w-[360px]">
          <div className="px-2.5 pb-1.5 pt-1 text-[12px] font-medium text-muted">
            Recent workspaces
          </div>
          {recent.length === 0 && <p className="px-2.5 py-2 text-[13px] text-faint">None yet.</p>}
          {recent.map((path) => (
            <button
              type="button"
              key={path}
              disabled={!canOpen}
              onClick={() => {
                set({ workspaceMenuOpen: false });
                void onOpenRecent(path);
              }}
              className={cn(
                "flex w-full cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-2 text-left hover:bg-s2",
                path === current && "bg-s2",
              )}
            >
              <Folder size={16} className="shrink-0 text-faint" />
              <span className="min-w-0 flex-1">
                <span className="block text-[14px] text-fg">{folderName(path)}</span>
                <span className="block truncate font-mono text-[11px] text-faint">{path}</span>
              </span>
              {path === current && <Check size={15} className="shrink-0 text-accent-text" />}
            </button>
          ))}
          <div className="mt-1 border-t border-line pt-1">
            <button
              type="button"
              disabled={!canOpen}
              onClick={() => {
                set({ workspaceMenuOpen: false });
                onOpenFolder();
              }}
              className="flex w-full cursor-pointer disabled:cursor-not-allowed disabled:opacity-45 items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-[14px] text-fg hover:bg-s2"
            >
              <FolderPlus size={16} className="text-muted" />
              Open folder…
            </button>
          </div>
        </PopoverContent>
      </Popover>
      {current && <span className="truncate font-mono text-[12px] text-faint">{current}</span>}
    </div>
  );
}
