import { Undo2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import type { FileChange } from "@/state/changes";

/** Confirms an undo, which overwrites anything edited since the turn. Keep changes has the focus. */
export function UndoDialog({
  open,
  files,
  busy,
  onKeep,
  onConfirm,
}: {
  open: boolean;
  files: FileChange[];
  busy: boolean;
  onKeep: () => void;
  onConfirm: () => void;
}) {
  const created = files.filter((f) => f.status === "added").length;
  const restored = files.length - created;
  const plural = (n: number) => `${n} ${n === 1 ? "file" : "files"}`;
  return (
    <Dialog open={open}>
      <DialogContent
        title="Undo the last turn?"
        onEscape={busy ? undefined : onKeep}
        className="w-[min(92vw,500px)]"
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          document.getElementById("undo-keep")?.focus();
        }}
      >
        <div className="px-6 pb-4 pt-6">
          <div className="flex items-center gap-3">
            <span className="grid h-10 w-10 place-items-center rounded-[10px] bg-warn-soft text-warn">
              <Undo2 size={19} />
            </span>
            <h2 className="text-[17px] font-semibold">Undo the last turn?</h2>
          </div>
          <p className="mt-3 text-[13.5px] leading-relaxed text-muted">
            This restores {plural(restored)} to how {restored === 1 ? "it was" : "they were"} before
            the turn and deletes {plural(created)} it created. Edits you made yourself since then
            are lost.
          </p>
          {files.length > 0 && (
            <ul
              aria-label="Files affected"
              className="mt-3 max-h-48 overflow-y-auto rounded-[10px] border border-line"
            >
              {files.map((f) => (
                <li key={f.path} className="flex h-8 items-center gap-3 px-3 text-[12.5px]">
                  <span className="min-w-0 flex-1 truncate font-mono text-fg">{f.path}</span>
                  <span className="text-[11px] text-muted">
                    {f.status === "added" ? "will be deleted" : "will be restored"}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className="flex justify-end gap-2 border-t border-line bg-s2 px-6 py-3.5">
          <Button id="undo-keep" variant="secondary" onClick={onKeep} disabled={busy}>
            Keep changes
          </Button>
          <Button variant="danger" onClick={onConfirm} disabled={busy}>
            {busy ? "Undoing…" : "Undo turn"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
