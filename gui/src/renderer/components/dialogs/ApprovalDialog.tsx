import { ShieldAlert } from "lucide-react";
import { useRef } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import type { Answer } from "@/lib/client";
import type { PendingAction } from "@/state/reduceTurn";

type Approval = Extract<PendingAction, { kind: "approval" }>;

function parse(raw: string): Record<string, unknown> | null {
  try {
    const value: unknown = JSON.parse(raw);
    return value && typeof value === "object" && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

/** What the tool is about to do, as one line: the command, else the path, else the raw arguments. */
export function describeCall(action: Approval): string {
  const args = parse(action.rawArgs);
  for (const key of ["command", "path"]) {
    const value = args?.[key];
    if (typeof value === "string" && value) return value;
  }
  return action.rawArgs;
}

/** One approval at a time, oldest first, with how many are waiting. Deny has the focus. */
export function ApprovalDialog({
  action,
  position,
  total,
  workspace,
  onAnswer,
}: {
  action: Approval | undefined;
  position: number;
  total: number;
  workspace: string;
  onAnswer: (action: Approval, answer: Answer) => void;
}) {
  const deny = useRef<HTMLButtonElement>(null);
  const isCommand = action?.name === "bash";
  return (
    <Dialog open={action !== undefined}>
      {action && (
        <DialogContent
          title="Approval required"
          description="The agent wants to run something that needs your approval."
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            deny.current?.focus();
          }}
        >
          <div className="px-6 pb-2 pt-6">
            <div className="flex items-center gap-3">
              <span className="grid h-10 w-10 place-items-center rounded-[10px] bg-warn-soft text-warn">
                <ShieldAlert size={20} />
              </span>
              <div className="min-w-0 flex-1">
                <h2 className="text-[17px] font-semibold">
                  {isCommand ? "Run this command?" : "Allow this action?"}
                </h2>
                <p className="text-[13px] text-muted">The turn is paused until you decide.</p>
              </div>
              {total > 1 && (
                <span className="shrink-0 text-[12px] text-muted">
                  {position} of {total}
                </span>
              )}
            </div>

            <div className="mt-5 rounded-[12px] border border-line-strong bg-bg">
              <div className="flex items-center justify-between gap-3 border-b border-line px-3.5 py-2 text-[12px] text-muted">
                <span className="font-medium text-fg">{action.name}</span>
                <span className="truncate font-mono" title={workspace}>
                  {workspace}
                </span>
              </div>
              <pre className="overflow-x-auto whitespace-pre-wrap px-3.5 py-3 font-mono text-[13px] text-fg">
                {isCommand && <span className="text-faint">$ </span>}
                {describeCall(action)}
              </pre>
            </div>

            <dl className="mt-4 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-[13px]">
              <dt className="text-muted">Matched rule</dt>
              <dd>
                <code className="rounded bg-s3 px-1.5 py-0.5 font-mono text-[12px]">
                  {action.rule}
                </code>
              </dd>
              <dt className="text-muted">Why you're asked</dt>
              <dd className="text-fg">A permission rule asks before this runs.</dd>
            </dl>
            <p className="mt-3 text-[12px] text-faint">
              "Allow for this session" skips this prompt for the same rule until you close the
              session.
            </p>
          </div>

          <div className="mt-3 flex items-center gap-2 border-t border-line bg-s2 px-6 py-3.5">
            <Button ref={deny} variant="dangerOutline" onClick={() => onAnswer(action, "deny")}>
              Deny
            </Button>
            <div className="ml-auto flex gap-2">
              <Button variant="outline" onClick={() => onAnswer(action, "allow_for_session")}>
                Allow for this session
              </Button>
              <Button variant="primary" onClick={() => onAnswer(action, "allow")}>
                Allow once
              </Button>
            </div>
          </div>
        </DialogContent>
      )}
    </Dialog>
  );
}
