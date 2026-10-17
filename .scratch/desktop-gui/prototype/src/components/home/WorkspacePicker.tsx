import { Check, ChevronDown, Folder, FolderPlus } from "lucide-react";
import { RestrictedChip } from "@/components/banners/RestrictedChip";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/cn";
import { WORKSPACES } from "@/mock/data";
import { useApp } from "@/state/store";

export function WorkspacePicker() {
  const { state, set } = useApp();
  const ws = WORKSPACES.find((w) => w.id === state.workspaceId) ?? WORKSPACES[0];
  const restricted = ws.hasConfig && !state.trustedWorkspaces.includes(ws.id);
  return (
    <div className="flex items-center gap-2 rounded-t-[18px] border border-b-0 border-line bg-s1 px-3.5 py-2">
      <Popover open={state.workspaceMenuOpen} onOpenChange={(workspaceMenuOpen) => set({ workspaceMenuOpen })}>
        <PopoverTrigger asChild>
          <button
            aria-label={`Workspace: ${ws.name}`}
            className="flex h-7 cursor-pointer items-center gap-2 rounded-md pr-1.5 text-[14px] text-fg hover:bg-s2"
          >
            <Folder size={16} className="text-accent-text" />
            {ws.name}
            <ChevronDown size={13} className="text-faint" />
          </button>
        </PopoverTrigger>
        <PopoverContent side="top" className="w-[360px]">
          <div className="px-2.5 pb-1.5 pt-1 text-[12px] font-medium text-muted">Recent workspaces</div>
          {WORKSPACES.map((w) => (
            <button
              key={w.id}
              onClick={() => set({ workspaceId: w.id, workspaceMenuOpen: false })}
              className={cn("flex w-full cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-2 text-left hover:bg-s2", w.id === ws.id && "bg-s2")}
            >
              <Folder size={16} className="shrink-0 text-faint" />
              <span className="min-w-0 flex-1">
                <span className="block text-[14px] text-fg">{w.name}</span>
                <span className="block truncate font-mono text-[11px] text-faint">{w.path}</span>
              </span>
              {w.hasConfig && !state.trustedWorkspaces.includes(w.id) && <span className="text-[11px] text-peel">restricted</span>}
              {w.id === ws.id && <Check size={15} className="shrink-0 text-accent-text" />}
            </button>
          ))}
          <div className="mt-1 border-t border-line pt-1">
            <button className="flex w-full cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-[14px] text-fg hover:bg-s2">
              <FolderPlus size={16} className="text-muted" />
              Open folder…
            </button>
          </div>
        </PopoverContent>
      </Popover>
      <span className="truncate font-mono text-[12px] text-faint">{ws.path}</span>
      {restricted && (
        <span className="ml-auto">
          <RestrictedChip />
        </span>
      )}
    </div>
  );
}
