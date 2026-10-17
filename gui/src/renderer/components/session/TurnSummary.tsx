import { ChevronDown, Undo2 } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/cn";
import type { TurnSummary as Summary } from "@/state/reduceTurn";

const SHOWN = 3;

function seconds(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  return total < 60 ? `${total} s` : `${Math.floor(total / 60)} min ${total % 60} s`;
}

/** What the turn did to files, with Undo for the last turn when it can be undone. */
export function TurnSummary({
  summary,
  undone,
  canUndo,
  undoing,
  undoError,
  onUndo,
  onViewChanges,
}: {
  summary: Summary;
  undone: boolean;
  canUndo: boolean;
  undoing: boolean;
  undoError: string | null;
  onUndo: () => void;
  onViewChanges: () => void;
}) {
  const [all, setAll] = useState(false);
  if (summary.files.length === 0 && !canUndo && !undone) return null;
  const shown = all ? summary.files : summary.files.slice(0, SHOWN);
  const count = summary.files.length;
  const struck = undone ? "line-through opacity-60" : "";
  return (
    <div className="overflow-hidden rounded-[14px] border border-line-strong bg-s1">
      <div className="flex items-center gap-3 px-4 py-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-[14px] font-medium">
            <span className={cn(struck)}>
              {count > 0 ? (
                <>
                  Edited {count} {count === 1 ? "file" : "files"}{" "}
                  <span className="ml-1 font-mono text-[12.5px] text-add-fg">+{summary.added}</span>{" "}
                  <span className="font-mono text-[12.5px] text-del-fg">−{summary.removed}</span>
                </>
              ) : (
                "This turn changed files"
              )}
            </span>
            {undone && <Badge tone="warn">Undone</Badge>}
          </div>
          {summary.durationMs !== null && (
            <div className="text-[12px] text-faint">{seconds(summary.durationMs)}</div>
          )}
        </div>
        {canUndo && (
          <Button variant="outline" size="sm" disabled={undoing} onClick={onUndo}>
            <Undo2 size={13} />
            {undoing ? "Undoing…" : "Undo"}
          </Button>
        )}
        <Button variant="secondary" size="sm" onClick={onViewChanges}>
          View changes
        </Button>
      </div>
      {undoError && (
        <p
          role="alert"
          className="border-t border-danger/40 bg-danger-soft px-4 py-2 text-[12.5px] text-danger"
        >
          Couldn't undo: {undoError}
        </p>
      )}
      {count > 0 && (
        <ul className="border-t border-line">
          {shown.map((f) => (
            <li key={f.path} className="flex h-9 items-center gap-3 px-4 hover:bg-s2">
              <span
                className={cn("min-w-0 flex-1 truncate font-mono text-[12.5px] text-fg", struck)}
              >
                {f.path}
              </span>
              <span className={cn("font-mono text-[12px]", struck)}>
                <span className="text-add-fg">+{f.added}</span>{" "}
                <span className="text-del-fg">−{f.removed}</span>
              </span>
            </li>
          ))}
        </ul>
      )}
      {count > SHOWN && (
        <button
          type="button"
          onClick={() => setAll(!all)}
          className="flex h-9 w-full cursor-pointer items-center gap-1.5 border-t border-line px-4 text-[13px] text-muted hover:bg-s2 hover:text-fg"
        >
          {all ? "Show fewer files" : `Show ${count - SHOWN} more files`}
          <ChevronDown size={14} className={all ? "rotate-180" : ""} />
        </button>
      )}
    </div>
  );
}
