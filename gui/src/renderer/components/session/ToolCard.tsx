import {
  Ban,
  Check,
  ChevronRight,
  FileEdit,
  FilePlus2,
  FileText,
  HelpCircle,
  Loader2,
  Search,
  ShieldQuestion,
  Terminal,
  TriangleAlert,
  Wrench,
} from "lucide-react";
import { type ReactNode, useState } from "react";
import { DiffView } from "@/components/changes/DiffView";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/cn";
import { clipOutput } from "@/state/output";
import type { ToolItem } from "@/state/reduceTurn";
import { useApp } from "@/state/store";

const ICONS: Record<string, ReactNode> = {
  read_file: <FileText size={15} />,
  grep: <Search size={15} />,
  glob: <Search size={15} />,
  edit: <FileEdit size={15} />,
  write_file: <FilePlus2 size={15} />,
  bash: <Terminal size={15} />,
  ask_question: <HelpCircle size={15} />,
};

/** The one line a closed card shows: the command, the path or the pattern, else the raw arguments. */
export function summaryOf(tool: ToolItem): string {
  const args = tool.args;
  if (tool.name === "ask_question" && typeof args?.question === "string") return args.question;
  if (!args) return tool.rawArgs;
  for (const key of ["command", "path", "pattern", "query", "name"]) {
    const value = args[key];
    if (typeof value === "string" && value) return value;
  }
  return tool.rawArgs;
}

export function ToolCard({ tool }: { tool: ToolItem }) {
  const { state, set } = useApp();
  const [open, setOpen] = useState(false);
  const [all, setAll] = useState(false);
  const blocked = tool.state === "blocked";
  const awaiting = tool.state === "awaiting_approval";
  const output = tool.output ?? "";
  const clip = clipOutput(output);
  const shown = all ? output : clip.text;
  const hasBody = Boolean(tool.diff) || output !== "";

  return (
    <div
      className={cn(
        "overflow-hidden rounded-[12px] border bg-s1",
        blocked
          ? "border-warn/40"
          : tool.state === "interrupted"
            ? "border-danger/40"
            : "border-line",
      )}
    >
      <button
        type="button"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        className="flex h-10 w-full cursor-pointer items-center gap-2.5 px-3 text-left hover:bg-s2"
      >
        <ChevronRight
          size={14}
          className={cn("shrink-0 text-faint transition-transform", open && "rotate-90")}
        />
        <span className={cn("shrink-0", blocked ? "text-warn" : "text-muted")}>
          {ICONS[tool.name] ?? <Wrench size={15} />}
        </span>
        <span className="shrink-0 text-[13px] font-medium text-fg">{tool.name}</span>
        <span className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-muted">
          {summaryOf(tool)}
        </span>
        {tool.diff && (
          <span className="shrink-0 font-mono text-[12px]">
            <span className="text-add-fg">+{tool.diff.added}</span>{" "}
            <span className="text-del-fg">−{tool.diff.removed}</span>
          </span>
        )}
        {tool.exitCode !== undefined && (
          <span className="shrink-0 font-mono text-[11px] text-muted">exit {tool.exitCode}</span>
        )}
        <span className="shrink-0">
          {tool.state === "done" && (
            <Check size={15} className="text-ok" aria-label="Done" role="img" />
          )}
          {tool.state === "running" && (
            <Loader2
              size={15}
              className="animate-spin text-accent-text"
              aria-label="Running"
              role="img"
            />
          )}
          {tool.state === "awaiting_approval" && (
            <ShieldQuestion
              size={15}
              className="text-warn"
              aria-label="Awaiting approval"
              role="img"
            />
          )}
          {tool.state === "blocked" && (
            <Ban size={15} className="text-warn" aria-label="Blocked" role="img" />
          )}
          {tool.state === "interrupted" && (
            <TriangleAlert size={15} className="text-danger" aria-label="Interrupted" role="img" />
          )}
        </span>
      </button>
      {blocked && (
        <div className="flex items-center gap-2 border-t border-warn/25 bg-warn-soft px-3.5 py-2 text-[12.5px] text-warn">
          <span>
            Blocked by guardrail{" "}
            <code className="rounded bg-s1/60 px-1.5 font-mono">{tool.rule}</code>. The command did
            not run.
          </span>
        </div>
      )}
      {awaiting && (
        <div className="border-t border-warn/25 bg-warn-soft px-3.5 py-2 text-[12.5px] text-warn">
          Waiting for your approval · <span className="font-mono">{tool.rule}</span>
        </div>
      )}
      {tool.name === "ask_question" && tool.state === "done" && output !== "" && (
        <div className="border-t border-line px-3.5 py-2 text-[12.5px] text-muted">
          Answer: <span className="text-fg">{output}</span>
        </div>
      )}
      {typeof tool.args?.path === "string" && tool.diff && (
        <div className="flex border-t border-line px-3 py-1.5">
          <Button
            variant="ghost"
            size="sm"
            onClick={() =>
              set({
                changesOpen: true,
                changesScope: "turn",
                selectedFile: { ...state.selectedFile, turn: tool.args?.path as string },
              })
            }
          >
            View in changes
          </Button>
        </div>
      )}
      {open && !blocked && hasBody && tool.name !== "ask_question" && (
        <div className="border-t border-line bg-bg/60">
          {tool.diff ? (
            <div className="max-h-72 overflow-y-auto py-1.5">
              <DiffView patch={diffPatch(tool)} />
            </div>
          ) : (
            <>
              <pre className="max-h-60 overflow-auto whitespace-pre-wrap px-4 py-3 font-mono text-[12.5px] leading-relaxed text-muted">
                {shown}
              </pre>
              {clip.clipped && !all && (
                <button
                  type="button"
                  onClick={() => setAll(true)}
                  className="w-full cursor-pointer border-t border-line px-4 py-2 text-left text-[12.5px] text-muted hover:bg-s2 hover:text-fg"
                >
                  Show all
                </button>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}

/** The diff's own lines again as a patch; the card already parsed them once for the counts. */
function diffPatch(tool: ToolItem): string {
  return (tool.diff?.lines ?? [])
    .map((l) => {
      if (l.kind === "hunk") return l.text;
      if (l.kind === "note") return `\\ ${l.text}`;
      return `${l.kind === "add" ? "+" : l.kind === "del" ? "-" : " "}${l.text}`;
    })
    .join("\n");
}
