import { ShieldAlert } from "lucide-react";
import { useRef } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { APPROVAL } from "@/mock/data";
import { useApp } from "@/state/store";

export function ApprovalDialog() {
  const { state, set } = useApp();
  const deny = useRef<HTMLButtonElement>(null);
  const close = () => set({ dialog: null });
  return (
    <Dialog open={state.dialog === "approval"} onOpenChange={(o) => !o && close()}>
      <DialogContent
        title="Approval required"
        description="The agent wants to run a command that needs your approval."
        hideClose
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
            <div>
              <h2 className="text-[17px] font-semibold">Run this command?</h2>
              <p className="text-[13px] text-muted">The turn is paused until you decide.</p>
            </div>
          </div>

          <div className="mt-5 rounded-[12px] border border-line-strong bg-bg">
            <div className="flex items-center justify-between border-b border-line px-3.5 py-2 text-[12px] text-muted">
              <span className="font-medium text-fg">{APPROVAL.tool}</span>
              <span className="font-mono">{APPROVAL.cwd}</span>
            </div>
            <pre className="overflow-x-auto px-3.5 py-3 font-mono text-[13px] text-fg">
              <span className="text-faint">$ </span>
              {APPROVAL.command}
            </pre>
          </div>

          <dl className="mt-4 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-[13px]">
            <dt className="text-muted">Matched rule</dt>
            <dd><code className="rounded bg-s3 px-1.5 py-0.5 font-mono text-[12px]">{APPROVAL.rule}</code></dd>
            <dt className="text-muted">Why you're asked</dt>
            <dd className="text-fg">{APPROVAL.reason}</dd>
          </dl>
          <p className="mt-3 text-[12px] text-faint">"Allow for this session" skips this prompt for the same rule until you close the session.</p>
        </div>

        <div className="mt-3 flex items-center gap-2 border-t border-line bg-s2 px-6 py-3.5">
          <Button ref={deny} variant="dangerOutline" onClick={close}>
            Deny
          </Button>
          <div className="ml-auto flex gap-2">
            <Button variant="outline" onClick={close}>
              Allow for this session
            </Button>
            <Button variant="primary" onClick={close}>
              Allow once
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
