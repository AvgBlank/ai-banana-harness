import { FileCode2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useApp } from "@/state/store";

/** The Changes panel's frame and empty state; the file list and diffs arrive with the real data. */
export function ChangesPlaceholder() {
  const { set } = useApp();
  const width = Math.min(480, Math.round(window.innerWidth * 0.4));
  return (
    <aside
      aria-label="Changes"
      className="relative flex shrink-0 flex-col border-l border-line bg-s1"
      style={{ width }}
    >
      <div className="flex h-11 shrink-0 items-center gap-2 border-b border-line px-3">
        <h2 className="text-[14px] font-medium">Changes</h2>
        <Button
          variant="ghost"
          size="iconSm"
          className="ml-auto"
          aria-label="Close changes"
          onClick={() => set({ changesOpen: false })}
        >
          <X size={16} />
        </Button>
      </div>
      <div className="grid flex-1 place-items-center px-8 text-center">
        <div>
          <FileCode2 size={28} className="mx-auto text-faint" />
          <p className="mt-3 text-[14px] font-medium">No changes yet</p>
          <p className="mt-1 text-[13px] text-muted">
            Files the agent edits or creates show up here with their diffs.
          </p>
        </div>
      </div>
    </aside>
  );
}
