import { ChevronDown, Undo2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { TURN_CHANGES } from "@/mock/data";
import { useApp } from "@/state/store";

export function TurnSummary() {
  const { set } = useApp();
  const [all, setAll] = useState(false);
  const added = TURN_CHANGES.reduce((n, f) => n + f.added, 0);
  const removed = TURN_CHANGES.reduce((n, f) => n + f.removed, 0);
  const shown = all ? TURN_CHANGES : TURN_CHANGES.slice(0, 3);
  return (
    <div className="overflow-hidden rounded-[14px] border border-line-strong bg-s1">
      <div className="flex items-center gap-3 px-4 py-3">
        <div className="min-w-0 flex-1">
          <div className="text-[14px] font-medium">
            Edited {TURN_CHANGES.length} files <span className="ml-1 font-mono text-[12.5px] text-add-fg">+{added}</span>{" "}
            <span className="font-mono text-[12.5px] text-del-fg">−{removed}</span>
          </div>
          <div className="text-[12px] text-faint">Checkpoint taken before the first change · 11 s</div>
        </div>
        <Button variant="outline" size="sm">
          <Undo2 size={13} />
          Undo
        </Button>
        <Button variant="secondary" size="sm" onClick={() => set({ changesOpen: true })}>
          View changes
        </Button>
      </div>
      <ul className="border-t border-line">
        {shown.map((f) => (
          <li key={f.path} className="flex h-9 items-center gap-3 px-4 hover:bg-s2">
            <span className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-fg">{f.path}</span>
            <span className="font-mono text-[12px]">
              <span className="text-add-fg">+{f.added}</span> <span className="text-del-fg">−{f.removed}</span>
            </span>
          </li>
        ))}
      </ul>
      {TURN_CHANGES.length > 3 && (
        <button
          onClick={() => setAll(!all)}
          className="flex h-9 w-full cursor-pointer items-center gap-1.5 border-t border-line px-4 text-[13px] text-muted hover:bg-s2 hover:text-fg"
        >
          {all ? "Show fewer files" : `Show ${TURN_CHANGES.length - 3} more files`}
          <ChevronDown size={14} className={all ? "rotate-180" : ""} />
        </button>
      )}
    </div>
  );
}
