import { ChevronDown, FileCode2, FileImage, FilePlus2, FileX2, RefreshCw, X } from "lucide-react";
import { useRef, useState } from "react";
import { DiffView } from "@/components/changes/DiffView";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/cn";
import type { ChangesEntry, FileChange } from "@/state/changes";
import type { ChangesScope } from "@/state/store";

const MIN_WIDTH = 360;
const MAX_WIDTH = 760;
const KEY_STEP = 24;

const SCOPES: Array<[ChangesScope, string, string]> = [
  ["turn", "Last turn", "Compared with the checkpoint taken before it"],
  ["workspace", "Workspace", "Everything uncommitted against HEAD"],
];

function FileIcon({ file }: { file: FileChange }) {
  if (isBinary(file)) return <FileImage size={15} className="text-faint" />;
  if (file.status === "added") return <FilePlus2 size={15} className="text-add-fg" />;
  if (file.status === "deleted") return <FileX2 size={15} className="text-del-fg" />;
  return <FileCode2 size={15} className="text-faint" />;
}

/** Git reports a binary file with no patch and no counts. */
function isBinary(file: FileChange): boolean {
  return file.patch === null && file.added + file.removed === 0 && !file.truncated;
}

export function totals(files: FileChange[]): { added: number; removed: number } {
  return {
    added: files.reduce((n, f) => n + f.added, 0),
    removed: files.reduce((n, f) => n + f.removed, 0),
  };
}

/** The file to show: the one selected by path while it is in the list, else the first. */
export function pickFile(files: FileChange[], path: string | null): FileChange | undefined {
  return files.find((f) => f.path === path) ?? files[0];
}

export function ChangesPanel({
  scope,
  entry,
  selectedPath,
  lastTurnFinished,
  width,
  onScope,
  onSelect,
  onResize,
  onRetry,
  onClose,
}: {
  scope: ChangesScope;
  entry: ChangesEntry;
  selectedPath: string | null;
  /** The session's last turn has ended, which decides how an empty last-turn list is explained. */
  lastTurnFinished: boolean;
  width: number;
  onScope: (scope: ChangesScope) => void;
  onSelect: (path: string) => void;
  onResize: (width: number) => void;
  onRetry: () => void;
  onClose: () => void;
}) {
  const [scopeOpen, setScopeOpen] = useState(false);
  const [dragWidth, setDragWidth] = useState<number | null>(null);
  const dragging = useRef(false);
  const files = entry.files;
  const selected = pickFile(files, selectedPath);
  const { added, removed } = totals(files);
  const shownWidth = dragWidth ?? width;
  const clamp = (value: number) => Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(value)));
  const loading = entry.status === "loading";

  return (
    <aside
      aria-label="Changes"
      aria-busy={loading}
      className="relative flex shrink-0 flex-col border-l border-line bg-s1"
      style={{ width: shownWidth }}
    >
      {/* biome-ignore lint/a11y/useSemanticElements: a draggable splitter has no native element */}
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize changes panel"
        aria-valuenow={shownWidth}
        aria-valuemin={MIN_WIDTH}
        aria-valuemax={MAX_WIDTH}
        tabIndex={0}
        onPointerDown={(e) => {
          dragging.current = true;
          e.currentTarget.setPointerCapture?.(e.pointerId);
        }}
        onPointerMove={(e) => {
          if (dragging.current) setDragWidth(clamp(window.innerWidth - e.clientX));
        }}
        onPointerUp={() => {
          dragging.current = false;
          // The width is saved once, when the drag ends, not on every move.
          if (dragWidth !== null) onResize(dragWidth);
          setDragWidth(null);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowLeft") onResize(clamp(width + KEY_STEP));
          if (e.key === "ArrowRight") onResize(clamp(width - KEY_STEP));
        }}
        className="absolute -left-1 top-0 z-10 h-full w-2 cursor-col-resize hover:bg-accent-soft"
      />
      {loading && (
        <div
          role="progressbar"
          aria-label="Loading changes"
          className="absolute inset-x-0 top-0 h-0.5 animate-pulse bg-accent"
        />
      )}

      <div className="flex items-center gap-2 border-b border-line px-3 py-2.5">
        <Popover open={scopeOpen} onOpenChange={setScopeOpen}>
          <PopoverTrigger asChild>
            <button
              type="button"
              aria-label={`Scope: ${scope === "turn" ? "Last turn" : "Workspace"}`}
              className="flex h-8 cursor-pointer items-center gap-1.5 rounded-full border border-line-strong bg-s2 pl-3.5 pr-2.5 text-[13px] font-medium hover:bg-s3"
            >
              {scope === "turn" ? "Last turn" : "Workspace"}
              <span className="ml-1 font-mono text-[12px] text-add-fg">+{added}</span>
              <span className="font-mono text-[12px] text-del-fg">−{removed}</span>
              <ChevronDown size={13} className="text-muted" />
            </button>
          </PopoverTrigger>
          <PopoverContent className="w-60">
            {SCOPES.map(([id, label, hint]) => (
              <button
                type="button"
                key={id}
                onClick={() => {
                  onScope(id);
                  setScopeOpen(false);
                }}
                className={cn(
                  "block w-full cursor-pointer rounded-md px-2.5 py-2 text-left hover:bg-s2",
                  scope === id && "bg-s2",
                )}
              >
                <span className="block text-[14px]">{label}</span>
                <span className="block text-[12px] text-faint">{hint}</span>
              </button>
            ))}
          </PopoverContent>
        </Popover>
        <Button
          variant="ghost"
          size="iconSm"
          className="ml-auto"
          aria-label="Refresh changes"
          onClick={onRetry}
        >
          <RefreshCw size={15} />
        </Button>
        <Button variant="ghost" size="iconSm" aria-label="Close changes" onClick={onClose}>
          <X size={16} />
        </Button>
      </div>

      {entry.status === "error" && (
        <div
          role="alert"
          className="flex items-start gap-3 border-b border-danger/40 bg-danger-soft px-3.5 py-2.5 text-[13px]"
        >
          <div className="min-w-0 flex-1">
            <div className="font-medium text-danger">Couldn't load changes</div>
            {entry.error && <div className="mt-0.5 break-words text-muted">{entry.error}</div>}
          </div>
          <Button variant="outline" size="sm" onClick={onRetry}>
            Retry
          </Button>
        </div>
      )}

      {files.length === 0 ? (
        <EmptyState scope={scope} entry={entry} lastTurnFinished={lastTurnFinished} />
      ) : (
        <>
          <ul
            className="max-h-[34%] shrink-0 overflow-y-auto border-b border-line p-1.5"
            aria-label="Changed files"
          >
            {files.map((f) => (
              <li key={f.path}>
                <button
                  type="button"
                  onClick={() => onSelect(f.path)}
                  aria-current={selected?.path === f.path ? "true" : undefined}
                  className={cn(
                    "flex h-8 w-full cursor-pointer items-center gap-2 rounded-md px-2 text-left hover:bg-s2",
                    selected?.path === f.path && "bg-s3",
                  )}
                >
                  <FileIcon file={f} />
                  <span className="min-w-0 flex-1 truncate font-mono text-[12.5px]" title={f.path}>
                    <span className="text-faint">
                      {f.path.includes("/") ? f.path.slice(0, f.path.lastIndexOf("/") + 1) : ""}
                    </span>
                    <span className="text-fg">{f.path.slice(f.path.lastIndexOf("/") + 1)}</span>
                  </span>
                  {f.status === "deleted" && (
                    <span className="text-[11px] text-del-fg">deleted</span>
                  )}
                  {f.status === "renamed" && (
                    <span className="text-[11px] text-muted">renamed</span>
                  )}
                  {isBinary(f) ? (
                    <span className="text-[11px] text-faint">binary</span>
                  ) : (
                    <span className="shrink-0 font-mono text-[12px]">
                      <span className="text-add-fg">+{f.added}</span>{" "}
                      <span className="text-del-fg">−{f.removed}</span>
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
                <span className="rounded-full bg-s3 px-2 py-0.5 text-[11px] text-muted">
                  {selected.status}
                </span>
              </div>
              <div className="min-h-0 flex-1 overflow-y-auto py-1.5">
                {selected.patch ? (
                  <DiffView patch={selected.patch} truncated={selected.truncated} />
                ) : (
                  <div className="px-4 py-6 text-[13px] text-muted">
                    {isBinary(selected)
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

function EmptyState({
  scope,
  entry,
  lastTurnFinished,
}: {
  scope: ChangesScope;
  entry: ChangesEntry;
  lastTurnFinished: boolean;
}) {
  let title = "No changes yet";
  let hint = "Files the agent edits or creates show up here with their diffs.";
  if (entry.status === "loading") {
    title = "Loading changes…";
    hint = "";
  } else if (entry.status === "error") {
    title = "";
    hint = "";
  } else if (scope === "workspace" && entry.git === false) {
    title = "Not a git repository";
    hint = "This workspace is not a git repository, so changes and undo are not available.";
  } else if (scope === "turn" && entry.checkpoint === false && lastTurnFinished) {
    title = "No changes in the last turn";
    hint = "The last turn didn't change any files.";
  }
  if (!title) return <div className="flex-1" />;
  return (
    <div className="grid flex-1 place-items-center px-8 text-center">
      <div>
        <FileCode2 size={28} className="mx-auto text-faint" />
        <p className="mt-3 text-[14px] font-medium">{title}</p>
        {hint && <p className="mt-1 text-[13px] text-muted">{hint}</p>}
      </div>
    </div>
  );
}
