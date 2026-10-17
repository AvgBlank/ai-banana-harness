import { ExternalLink, Power } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { useApp } from "@/state/store";

export function QuitDialog() {
  const { state, set } = useApp();
  const close = () => set({ dialog: null });
  return (
    <Dialog open={state.dialog === "quit"} onOpenChange={(o) => !o && close()}>
      <DialogContent title="Quit while a turn is running?" hideClose className="w-[min(92vw,460px)]">
        <div className="px-6 pb-4 pt-6">
          <div className="flex items-center gap-3">
            <span className="grid h-10 w-10 place-items-center rounded-[10px] bg-warn-soft text-warn">
              <Power size={19} />
            </span>
            <h2 className="text-[17px] font-semibold">A turn is still running</h2>
          </div>
          <p className="mt-3 text-[13.5px] leading-relaxed text-muted">
            Quitting cancels it. Files it already changed stay as they are, and you can undo them from the session next time you open the app.
          </p>
        </div>
        <div className="flex justify-end gap-2 border-t border-line bg-s2 px-6 py-3.5">
          <Button variant="secondary" onClick={close} autoFocus>
            Keep working
          </Button>
          <Button variant="danger" onClick={close}>
            Quit and cancel turn
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export function LinkDialog() {
  const { state, set } = useApp();
  const [remember, setRemember] = useState(false);
  const close = () => set({ dialog: null });
  const url = "https://platform.deepseek.com/api_keys";
  return (
    <Dialog open={state.dialog === "link"} onOpenChange={(o) => !o && close()}>
      <DialogContent title="Open link in your browser?" hideClose className="w-[min(92vw,500px)]">
        <div className="px-6 pb-4 pt-6">
          <div className="flex items-center gap-3">
            <span className="grid h-10 w-10 place-items-center rounded-[10px] bg-s3 text-muted">
              <ExternalLink size={18} />
            </span>
            <h2 className="text-[17px] font-semibold">Open link in your browser?</h2>
          </div>
          <p className="mt-3 text-[13.5px] text-muted">This link came from the agent's output.</p>
          <code className="mt-2 block break-all rounded-md border border-line bg-bg px-3 py-2 font-mono text-[12.5px]">{url}</code>
          <label className="mt-3 flex cursor-pointer items-center gap-2.5 text-[13px] text-muted">
            <Checkbox checked={remember} onCheckedChange={(v) => setRemember(v === true)} aria-label="Don't ask again for this host" />
            Don't ask again for platform.deepseek.com this session
          </label>
        </div>
        <div className="flex justify-end gap-2 border-t border-line bg-s2 px-6 py-3.5">
          <Button variant="secondary" onClick={close} autoFocus>
            Cancel
          </Button>
          <Button variant="primary" onClick={close}>
            Open in browser
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
