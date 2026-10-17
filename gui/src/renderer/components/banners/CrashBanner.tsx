import { TriangleAlert } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { MAX_RESTARTS, type SidecarState } from "../../../shared/api";

/** Under the title bar while the sidecar is restarting, or stopped for good and waiting for Restart. */
export function CrashBanner({
  state,
  appName,
  onRestart,
}: {
  state: SidecarState;
  appName: string;
  onRestart: () => void;
}) {
  const [details, setDetails] = useState(false);
  if (state.state !== "crashed" && state.state !== "failed") return null;
  const failed = state.state === "failed";
  return (
    <div
      role="alert"
      className="flex shrink-0 flex-col gap-1 border-b border-warn/40 bg-warn-soft px-4 py-2 text-[13px] text-warn"
    >
      <div className="flex items-center gap-3">
        <TriangleAlert size={15} className="shrink-0" />
        <span className="min-w-0 flex-1">
          {failed
            ? `${appName} stopped and could not restart. A running turn was interrupted.`
            : `${appName} stopped. Restarting… attempt ${state.attempt} of ${MAX_RESTARTS}. A running turn was interrupted.`}
        </span>
        {failed && (
          <>
            {state.message && (
              <button
                type="button"
                onClick={() => setDetails(!details)}
                className="cursor-pointer text-[12.5px] underline"
              >
                {details ? "Hide details" : "Show details"}
              </button>
            )}
            <Button variant="primary" size="sm" onClick={onRestart}>
              Restart
            </Button>
          </>
        )}
      </div>
      {failed && details && (
        <pre className="max-h-40 overflow-auto whitespace-pre-wrap rounded-md bg-s1 p-2 font-mono text-[11.5px] text-fg">
          {state.message}
        </pre>
      )}
    </div>
  );
}
