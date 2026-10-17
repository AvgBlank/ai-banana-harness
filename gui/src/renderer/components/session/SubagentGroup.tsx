import { Bot, ChevronRight, Loader2, TriangleAlert } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/cn";
import { groupStatus, type Item, type ToolItem, type TurnView } from "@/state/reduceTurn";
import { RichText } from "./RichText";
import { ToolCard } from "./ToolCard";

/** A list of items: text, tool cards (a `create_sub_agent` card becomes a group) and notices. */
export function ItemList({
  items,
  turn,
  streaming,
}: {
  items: Item[];
  turn: TurnView;
  streaming: boolean;
}) {
  const last = items.length - 1;
  return (
    <>
      {items.map((item, i) => {
        if (item.kind === "assistant") {
          return (
            <p
              key={`a-${item.threadId}-${item.messageId}`}
              className="whitespace-pre-wrap text-[14.5px] leading-relaxed text-fg/90"
            >
              <RichText text={item.text} />
              {streaming && i === last && !item.final && <span className="caret" aria-hidden />}
            </p>
          );
        }
        if (item.kind === "notice") {
          return (
            <div key={`n-${item.id}`} className="flex items-center gap-3 text-[12px] text-faint">
              <span className="h-px flex-1 bg-line" />
              <span>{item.text}</span>
              <span className="h-px flex-1 bg-line" />
            </div>
          );
        }
        return item.name === "create_sub_agent" ? (
          <SubagentGroup
            key={`t-${item.toolCallId}`}
            tool={item}
            turn={turn}
            streaming={streaming}
          />
        ) : (
          <ToolCard key={`t-${item.toolCallId}`} tool={item} />
        );
      })}
    </>
  );
}

export function SubagentGroup({
  tool,
  turn,
  streaming,
}: {
  tool: ToolItem;
  turn: TurnView;
  streaming: boolean;
}) {
  const [open, setOpen] = useState(true);
  const status = groupStatus(turn, tool);
  const title = typeof tool.args?.name === "string" ? tool.args.name : "Subagent";
  const calls = tool.children.reduce(
    (n, id) => n + (turn.threads[id]?.items.filter((i) => i.kind === "tool").length ?? 0),
    0,
  );
  return (
    <div className="overflow-hidden rounded-[12px] border border-line bg-s1">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        className="flex w-full cursor-pointer items-center gap-2.5 px-3 py-2.5 text-left hover:bg-s2"
      >
        <ChevronRight
          size={14}
          className={cn("shrink-0 text-faint transition-transform", open && "rotate-90")}
        />
        <span className="grid h-6 w-6 place-items-center rounded-md bg-peel-soft text-peel">
          <Bot size={14} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13px] font-medium">{title}</div>
          <div className="text-[12px] text-faint">
            Subagent · {calls} tool call{calls === 1 ? "" : "s"}
          </div>
        </div>
        {status === "running" && (
          <Loader2
            size={15}
            className="animate-spin text-accent-text"
            aria-label="Running"
            role="img"
          />
        )}
        {status === "done" && <Badge tone="ok">done</Badge>}
        {status === "error" && (
          <Badge tone="danger">
            <TriangleAlert size={11} />
            failed
          </Badge>
        )}
        {status === "interrupted" && <Badge tone="warn">interrupted</Badge>}
      </button>
      {open && tool.children.length > 0 && (
        <div className="flex flex-col gap-3 border-t border-line bg-bg/40 px-3 py-3">
          {tool.children.map((id) => {
            const thread = turn.threads[id];
            if (!thread) return null;
            return (
              <section
                key={id}
                aria-label={`Subagent ${thread.title}`}
                className="flex flex-col gap-3"
              >
                <ItemList items={thread.items} turn={turn} streaming={streaming} />
                {thread.error && <p className="text-[12.5px] text-danger">{thread.error}</p>}
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}
