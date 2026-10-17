import { ChevronDown } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import type { TurnSummary as Summary } from "@/state/reduceTurn";

const SHOWN = 3;

function seconds(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  return total < 60 ? `${total} s` : `${Math.floor(total / 60)} min ${total % 60} s`;
}

/** "Edited N files +A −R" for a finished turn; it renders nothing when no file was edited. */
export function TurnSummary({
  summary,
  onViewChanges,
}: {
  summary: Summary;
  onViewChanges: () => void;
}) {
  const [all, setAll] = useState(false);
  if (summary.files.length === 0) return null;
  const shown = all ? summary.files : summary.files.slice(0, SHOWN);
  const count = summary.files.length;
  return (
    <div className="overflow-hidden rounded-[14px] border border-line-strong bg-s1">
      <div className="flex items-center gap-3 px-4 py-3">
        <div className="min-w-0 flex-1">
          <div className="text-[14px] font-medium">
            Edited {count} {count === 1 ? "file" : "files"}{" "}
            <span className="ml-1 font-mono text-[12.5px] text-add-fg">+{summary.added}</span>{" "}
            <span className="font-mono text-[12.5px] text-del-fg">−{summary.removed}</span>
          </div>
          {summary.durationMs !== null && (
            <div className="text-[12px] text-faint">{seconds(summary.durationMs)}</div>
          )}
        </div>
        <Button variant="secondary" size="sm" onClick={onViewChanges}>
          View changes
        </Button>
      </div>
      <ul className="border-t border-line">
        {shown.map((f) => (
          <li key={f.path} className="flex h-9 items-center gap-3 px-4 hover:bg-s2">
            <span className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-fg">
              {f.path}
            </span>
            <span className="font-mono text-[12px]">
              <span className="text-add-fg">+{f.added}</span>{" "}
              <span className="text-del-fg">−{f.removed}</span>
            </span>
          </li>
        ))}
      </ul>
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
