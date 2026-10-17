import { Ban, Check, ChevronRight, FileEdit, FilePlus2, FileText, Loader2, Search, Terminal, TriangleAlert } from "lucide-react";
import { useState, type ReactNode } from "react";
import { DiffView } from "@/components/changes/DiffView";
import { cn } from "@/lib/cn";
import type { ToolCardData } from "@/mock/data";

const ICONS: Record<ToolCardData["name"], ReactNode> = {
  read_file: <FileText size={15} />,
  grep: <Search size={15} />,
  glob: <Search size={15} />,
  edit: <FileEdit size={15} />,
  write_file: <FilePlus2 size={15} />,
  bash: <Terminal size={15} />,
};

export function ToolCard({ card, defaultOpen = false }: { card: ToolCardData; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  const isDiff = (card.name === "edit" || card.name === "write_file") && card.detail?.startsWith("@@");
  const blocked = card.state === "blocked";

  return (
    <div
      className={cn(
        "overflow-hidden rounded-[12px] border bg-s1",
        blocked ? "border-warn/40" : card.state === "error" ? "border-danger/40" : "border-line",
      )}
    >
      <button
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        className="flex h-10 w-full cursor-pointer items-center gap-2.5 px-3 text-left hover:bg-s2"
      >
        <ChevronRight size={14} className={cn("shrink-0 text-faint transition-transform", open && "rotate-90")} />
        <span className={cn("shrink-0", blocked ? "text-warn" : "text-muted")}>{ICONS[card.name]}</span>
        <span className="shrink-0 text-[13px] font-medium text-fg">{card.name}</span>
        <span className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-muted">{card.summary}</span>
        {card.added !== undefined && (
          <span className="shrink-0 font-mono text-[12px]">
            <span className="text-add-fg">+{card.added}</span> <span className="text-del-fg">−{card.removed ?? 0}</span>
          </span>
        )}
        {card.exitCode !== undefined && card.state === "done" && <span className="shrink-0 font-mono text-[11px] text-muted">exit {card.exitCode}</span>}
        <span className="shrink-0">
          {card.state === "done" && <Check size={15} className="text-ok" />}
          {card.state === "running" && <Loader2 size={15} className="animate-spin text-accent-text" />}
          {card.state === "blocked" && <Ban size={15} className="text-warn" />}
          {card.state === "error" && <TriangleAlert size={15} className="text-danger" />}
        </span>
      </button>
      {blocked && (
        <div className="flex items-center gap-2 border-t border-warn/25 bg-warn-soft px-3.5 py-2 text-[12.5px] text-warn">
          Blocked by guardrail <code className="rounded bg-s1/60 px-1.5 font-mono">{card.rule}</code>. The command did not run.
        </div>
      )}
      {open && card.detail && !blocked && (
        <div className="border-t border-line bg-bg/60">
          {isDiff ? (
            <div className="max-h-72 overflow-y-auto py-1.5">
              <DiffView patch={card.detail} />
            </div>
          ) : (
            <pre className="max-h-60 overflow-auto whitespace-pre-wrap px-4 py-3 font-mono text-[12.5px] leading-relaxed text-muted">{card.detail}</pre>
          )}
        </div>
      )}
    </div>
  );
}
