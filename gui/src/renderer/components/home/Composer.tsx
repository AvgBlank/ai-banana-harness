import { ArrowUp, Plus, ShieldCheck } from "lucide-react";
import { ModelPicker } from "@/components/home/ModelPicker";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/cn";
import { useApp } from "@/state/store";

export function Composer({
  disabled,
  disabledReason,
  provider,
  onPickProvider,
  onSend,
  placeholder = "Describe a task. Enter to send, Shift+Enter for a new line.",
}: {
  disabled?: boolean;
  disabledReason?: string;
  provider: string | null;
  onPickProvider: (id: string) => void;
  onSend?: (text: string) => void;
  placeholder?: string;
}) {
  const { state, set } = useApp();
  const text = state.draft;
  const send = () => {
    if (disabled || !text.trim()) return;
    onSend?.(text.trim());
    set({ draft: "" });
  };
  return (
    <div
      className={cn(
        "@container rounded-[18px] border bg-s2 shadow-[0_1px_0_rgba(255,255,255,0.02)] transition-colors focus-within:border-accent-line",
        disabled ? "border-line opacity-80" : "border-line-strong",
      )}
    >
      <textarea
        value={text}
        onChange={(e) => set({ draft: e.target.value })}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            send();
          }
        }}
        disabled={disabled}
        rows={2}
        aria-label="Task"
        placeholder={disabled ? (disabledReason ?? placeholder) : placeholder}
        className="block max-h-48 min-h-[68px] w-full resize-none bg-transparent px-4 pb-1 pt-3.5 text-[15px] text-fg placeholder:text-faint focus:outline-none disabled:cursor-not-allowed"
      />
      <div className="flex items-center gap-1 px-2.5 pb-2.5 pt-1">
        <Tooltip label="Attach a file from the workspace">
          <button
            type="button"
            aria-label="Attach"
            disabled={disabled}
            className="grid h-8 w-8 cursor-pointer place-items-center rounded-md text-muted hover:bg-s3 hover:text-fg disabled:opacity-40"
          >
            <Plus size={18} />
          </button>
        </Tooltip>
        <Tooltip label="Risky actions ask for your approval before they run.">
          <span className="flex h-8 items-center gap-1.5 rounded-md px-2 text-[13px] text-muted">
            <ShieldCheck size={15} />
            <span className="hidden @md:inline">Ask for risky actions</span>
          </span>
        </Tooltip>
        <div className="ml-auto flex items-center gap-1">
          <ModelPicker provider={provider} onPick={onPickProvider} />
          <button
            type="button"
            aria-label="Send"
            onClick={send}
            disabled={disabled || !text.trim()}
            className="grid h-8 w-8 cursor-pointer place-items-center rounded-full bg-accent text-accent-fg hover:bg-accent-hover disabled:cursor-not-allowed disabled:bg-s3 disabled:text-faint"
          >
            <ArrowUp size={17} strokeWidth={2.5} />
          </button>
        </div>
      </div>
    </div>
  );
}
