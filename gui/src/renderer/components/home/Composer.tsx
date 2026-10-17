import { ArrowUp, Plus, ShieldCheck } from "lucide-react";
import { ModelPicker } from "@/components/home/ModelPicker";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/cn";
import { useApp } from "@/state/store";

export function Composer({
  disabled,
  disabledReason,
  running,
  stopping,
  provider,
  onPickProvider,
  onSend,
  onStop,
  placeholder = "Describe a task. Enter to send, Shift+Enter for a new line.",
}: {
  disabled?: boolean;
  disabledReason?: string;
  /** A turn is running in this workspace: typing is allowed, sending is not. */
  running?: boolean;
  stopping?: boolean;
  provider: string | null;
  onPickProvider: (id: string) => void;
  /** Rejects when the turn could not be started; the text then comes back with the error. */
  onSend?: (text: string) => Promise<void>;
  onStop?: () => void;
  placeholder?: string;
}) {
  const { state, set } = useApp();
  const error = state.sendError;
  const text = state.draft;
  const blocked = disabled || running;

  const send = async () => {
    const sent = text.trim();
    if (blocked || !sent || !onSend) return;
    set({ draft: "", sendError: null });
    try {
      await onSend(sent);
    } catch (failure) {
      set({
        draft: sent,
        sendError: failure instanceof Error ? failure.message : "The turn could not be started.",
      });
    }
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
            void send();
          }
        }}
        disabled={disabled}
        rows={2}
        aria-label="Task"
        placeholder={
          running
            ? "A turn is running in this workspace"
            : disabled
              ? (disabledReason ?? placeholder)
              : placeholder
        }
        className="block max-h-48 min-h-[68px] w-full resize-none bg-transparent px-4 pb-1 pt-3.5 text-[15px] text-fg placeholder:text-faint focus:outline-none disabled:cursor-not-allowed"
      />
      {error && (
        <p role="alert" className="px-4 pb-1 text-[12px] text-danger">
          {error}
        </p>
      )}
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
          {running ? (
            <Tooltip label={stopping ? "Stopping…" : "Stop the running turn"}>
              <button
                type="button"
                aria-label={stopping ? "Stopping…" : "Stop"}
                disabled={stopping}
                onClick={onStop}
                className="grid h-8 w-8 cursor-pointer place-items-center rounded-full bg-fg text-bg hover:opacity-85 disabled:opacity-50"
              >
                <span className="h-3 w-3 rounded-[3px] bg-bg" />
              </button>
            </Tooltip>
          ) : (
            <button
              type="button"
              aria-label="Send"
              onClick={() => void send()}
              disabled={blocked || !text.trim()}
              className="grid h-8 w-8 cursor-pointer place-items-center rounded-full bg-accent text-accent-fg hover:bg-accent-hover disabled:cursor-not-allowed disabled:bg-s3 disabled:text-faint"
            >
              <ArrowUp size={17} strokeWidth={2.5} />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
