import { ChevronDown, CircleAlert, RefreshCw, Loader2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { APP_NAME, STDERR_TAIL } from "@/mock/data";
import { useApp } from "@/state/store";

export function CrashBanner() {
  const { state, set } = useApp();
  const [details, setDetails] = useState(false);
  if (state.sidecar === "ready") return null;
  const failed = state.sidecar === "failed";

  return (
    <div role="alert" className="slide-down border-b border-danger/35 bg-danger-soft">
      <div className="flex items-center gap-3 px-4 py-2.5 text-[13px]">
        {failed ? <CircleAlert size={16} className="shrink-0 text-danger" /> : <Loader2 size={16} className="shrink-0 animate-spin text-danger" />}
        <div className="min-w-0 flex-1">
          <span className="font-medium text-fg">{failed ? `${APP_NAME} stopped and could not restart.` : `${APP_NAME} stopped. Restarting… attempt 2 of 3.`}</span>{" "}
          <span className="text-muted">The running turn was interrupted. Your sessions and changes are safe.</span>
        </div>
        <button
          onClick={() => setDetails(!details)}
          aria-expanded={details}
          className="flex cursor-pointer items-center gap-1 text-[12px] text-muted hover:text-fg"
        >
          Details <ChevronDown size={13} className={details ? "rotate-180" : ""} />
        </button>
        {failed ? (
          <Button variant="primary" size="sm" onClick={() => set({ sidecar: "ready" })}>
            <RefreshCw size={13} />
            Restart
          </Button>
        ) : (
          <Button variant="outline" size="sm" onClick={() => set({ sidecar: "failed" })}>
            Stop retrying
          </Button>
        )}
      </div>
      {details && (
        <pre className="max-h-40 overflow-auto border-t border-danger/25 bg-bg/70 px-4 py-3 font-mono text-[12px] leading-relaxed text-muted">{STDERR_TAIL}</pre>
      )}
    </div>
  );
}
