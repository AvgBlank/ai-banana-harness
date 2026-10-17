import { ChevronDown, FileCode2, FileImage, FilePlus2, X } from "lucide-react";
import { useRef, useState } from "react";
import { DiffView } from "@/components/changes/DiffView";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/cn";
import { TURN_CHANGES, WORKSPACE_CHANGES, type FileChange } from "@/mock/data";
import { useApp } from "@/state/store";

function FileIcon({ f }: { f: FileChange }) {
  if (f.path.endsWith(".png")) return <FileImage size={15} className="text-faint" />;
  if (f.status === "added") return <FilePlus2 size={15} className="text-add-fg" />;
  return <FileCode2 size={15} className="text-faint" />;
}

export function ChangesPanel({ empty = false }: { empty?: boolean }) {
  const { state, set } = useApp();
  const [width, setWidth] = useState(() => Math.min(480, Math.round(window.innerWidth * 0.4)));
  const [scopeOpen, setScopeOpen] = useState(false);
  const dragging = useRef(false);

  const files = empty ? [] : state.changesScope === "turn" ? TURN_CHANGES : WORKSPACE_CHANGES;
  const selected = files.find((f) => f.path === state.selectedFile) ?? files[0];
  const added = files.reduce((n, f) => n + f.added, 0);
  const removed = files.reduce((n, f) => n + f.removed, 0);

  return (
    <aside aria-label="Changes" className="relative flex shrink-0 flex-col border-l border-line bg-s1" style={{ width }}>
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize changes panel"
        tabIndex={0}
        onPointerDown={(e) => {
          dragging.current = true;
          (e.target as HTMLElement).setPointerCapture(e.pointerId);
        }}
        onPointerMove={(e) => {
          if (dragging.current) setWidth(Math.min(760, Math.max(360, window.innerWidth - e.clientX)));
        }}
        onPointerUp={() => (dragging.current = false)}
        onKeyDown={(e) => {
          if (e.key === "ArrowLeft") setWidth((w) => Math.min(760, w + 24));
          if (e.key === "ArrowRight") setWidth((w) => Math.max(360, w - 24));
        }}
        className="absolute -left-1 top-0 z-10 h-full w-2 cursor-col-resize hover:bg-accent-soft"
      />

      <div className="flex items-center gap-2 border-b border-line px-3 py-2.5">
        <Popover open={scopeOpen} onOpenChange={setScopeOpen}>
          <PopoverTrigger asChild>
            <button className="flex h-8 cursor-pointer items-center gap-1.5 rounded-full border border-line-strong bg-s2 pl-3.5 pr-2.5 text-[13px] font-medium hover:bg-s3">
              {state.changesScope === "turn" ? "Last turn" : "Workspace"}
              <span className="ml-1 font-mono text-[12px] text-add-fg">+{added}</span>
              <span className="font-mono text-[12px] text-del-fg">−{removed}</span>
              <ChevronDown size={13} className="text-muted" />
            </button>
          </PopoverTrigger>
          <PopoverContent className="w-60">
            {(
              [
                ["turn", "Last turn", "Compared with the checkpoint taken before it"],
                ["workspace", "Workspace", "Everything uncommitted against HEAD"],
              ] as const
            ).map(([id, label, hint]) => (
              <button
                key={id}
                onClick={() => {
                  set({ changesScope: id });
                  setScopeOpen(false);
                }}
                className={cn("block w-full cursor-pointer rounded-md px-2.5 py-2 text-left hover:bg-s2", state.changesScope === id && "bg-s2")}
              >
                <span className="block text-[14px]">{label}</span>
                <span className="block text-[12px] text-faint">{hint}</span>
              </button>
            ))}
          </PopoverContent>
        </Popover>
        <Button variant="ghost" size="iconSm" className="ml-auto" aria-label="Close changes" onClick={() => set({ changesOpen: false })}>
          <X size={16} />
        </Button>
      </div>

      {files.length === 0 ? (
        <div className="grid flex-1 place-items-center px-8 text-center">
          <div>
            <FileCode2 size={28} className="mx-auto text-faint" />
            <p className="mt-3 text-[14px] font-medium">No changes yet</p>
            <p className="mt-1 text-[13px] text-muted">Files the agent edits or creates show up here with their diffs.</p>
          </div>
        </div>
      ) : (
        <>
          <ul className="max-h-[34%] shrink-0 overflow-y-auto border-b border-line p-1.5" aria-label="Changed files">
            {files.map((f) => (
              <li key={f.path}>
                <button
                  onClick={() => set({ selectedFile: f.path })}
                  aria-current={selected?.path === f.path ? "true" : undefined}
                  className={cn(
                    "flex h-8 w-full cursor-pointer items-center gap-2 rounded-md px-2 text-left hover:bg-s2",
                    selected?.path === f.path && "bg-s3",
                  )}
                >
                  <FileIcon f={f} />
                  <span className="min-w-0 flex-1 truncate font-mono text-[12.5px]" title={f.path}>
                    <span className="text-faint">{f.path.includes("/") ? f.path.slice(0, f.path.lastIndexOf("/") + 1) : ""}</span>
                    <span className="text-fg">{f.path.slice(f.path.lastIndexOf("/") + 1)}</span>
                  </span>
                  {f.patch === null && f.added + f.removed === 0 ? (
                    <span className="text-[11px] text-faint">binary</span>
                  ) : (
                    <span className="shrink-0 font-mono text-[12px]">
                      <span className="text-add-fg">+{f.added}</span> <span className="text-del-fg">−{f.removed}</span>
                    </span>
                  )}
                </button>
              </li>
            ))}
          </ul>

          {selected && (
            <div className="flex min-h-0 flex-1 flex-col">
              <div className="flex items-center gap-2 border-b border-line px-3.5 py-2 text-[12.5px]">
                <span className="min-w-0 flex-1 truncate font-mono text-fg">{selected.path}</span>
                <span className="rounded-full bg-s3 px-2 py-0.5 text-[11px] text-muted">{selected.status}</span>
              </div>
              <div className="min-h-0 flex-1 overflow-y-auto py-1.5">
                {selected.patch ? (
                  <DiffView patch={selected.patch} truncated={selected.truncated} />
                ) : (
                  <div className="px-4 py-6 text-[13px] text-muted">
                    {selected.path.endsWith(".png")
                      ? "Binary file. Counts are not available and no patch is shown."
                      : selected.truncated
                        ? "This patch was too large to show. It was cut at 256 KiB."
                        : "No patch available for this file."}
                  </div>
                )}
              </div>
            </div>
          )}
        </>
      )}
    </aside>
  );
}
