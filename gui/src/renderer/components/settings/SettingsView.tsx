import { Check, Monitor, Moon, Sun } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/cn";
import { folderName } from "@/lib/format";
import { useClient } from "@/state/client-context";
import { type Theme, useApp } from "@/state/store";
import type { OpenedWorkspace, TrustedWorkspace } from "../../../shared/api";

/** Fixed colours on purpose: each card previews its own theme regardless of the active one. */
const PREVIEWS: Record<
  Theme,
  { bg: string; s1: string; line: string; accent: string; add: string }
> = {
  dark: { bg: "#1a1914", s1: "#211f19", line: "#3a362c", accent: "#ffd60a", add: "#2f8a52" },
  light: { bg: "#fbf6e4", s1: "#fffdf3", line: "#e2d8b3", accent: "#ffd60a", add: "#3a9a5b" },
  system: {
    bg: "linear-gradient(90deg,#1a1914 50%,#fbf6e4 50%)",
    s1: "",
    line: "#8a8368",
    accent: "#ffd60a",
    add: "#3a9a5b",
  },
};

function ThemeCard({ id, label, icon }: { id: Theme; label: string; icon: ReactNode }) {
  const { state, set } = useApp();
  const p = PREVIEWS[id];
  const active = state.theme === id;
  return (
    <button
      type="button"
      onClick={() => set({ theme: id })}
      aria-pressed={active}
      className={cn(
        "group cursor-pointer rounded-[14px] border p-2 text-left transition-colors",
        active
          ? "border-accent bg-accent-soft"
          : "border-line hover:border-line-strong hover:bg-s2",
      )}
    >
      <div
        className="h-[84px] overflow-hidden rounded-[9px] border"
        style={{ background: p.bg, borderColor: p.line }}
      >
        <div className="flex h-full">
          <div
            className="w-[22%] border-r p-1.5"
            style={{ borderColor: p.line, background: id === "system" ? "transparent" : p.s1 }}
          >
            <div className="mb-1 h-1.5 w-3/4 rounded-full" style={{ background: p.accent }} />
            <div className="mb-1 h-1.5 w-full rounded-full" style={{ background: p.line }} />
            <div className="h-1.5 w-2/3 rounded-full" style={{ background: p.line }} />
          </div>
          <div className="flex-1 space-y-1 p-2">
            <div className="h-1.5 w-4/5 rounded-full" style={{ background: p.line }} />
            <div className="h-1.5 w-3/5 rounded-full" style={{ background: p.line }} />
            <div
              className="mt-2 h-3 w-4/5 rounded-[3px]"
              style={{ background: p.add, opacity: 0.45 }}
            />
            <div className="ml-auto h-3 w-3 rounded-full" style={{ background: p.accent }} />
          </div>
        </div>
      </div>
      <div className="mt-2 flex items-center gap-2 px-1 text-[13px] font-medium">
        <span className="text-muted">{icon}</span>
        {label}
        {active && <Check size={14} className="ml-auto text-accent-text" />}
      </div>
    </button>
  );
}

/** Workspaces whose config the user trusted, each with Forget (the workspace goes back to restricted). */
function TrustedWorkspaces({
  onForgotten,
}: {
  onForgotten: (opened: OpenedWorkspace | null) => void;
}) {
  const client = useClient();
  const [list, setList] = useState<TrustedWorkspace[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(
    () =>
      client.trustedWorkspaces().then(
        (found) => setList(found),
        () => setError("The list could not be read."),
      ),
    [client],
  );
  useEffect(() => {
    void load();
  }, [load]);

  const forget = async (workspace: string) => {
    setError(null);
    try {
      onForgotten(await client.forgetTrust(workspace));
      await load();
    } catch (e) {
      setError(e instanceof Object && "message" in e ? String(e.message) : "Could not forget it.");
    }
  };

  return (
    <section className="border-b border-line py-7 last:border-b-0">
      <h2 className="text-[15px] font-semibold">Trusted workspaces</h2>
      <p className="mt-0.5 text-[13px] text-muted">
        Folders whose config you trusted. Trust is tied to the file's content: a change asks again.
      </p>
      {error && (
        <p role="alert" className="mt-3 text-[13px] text-danger">
          {error}
        </p>
      )}
      {list?.length === 0 && (
        <p className="mt-4 text-[13px] text-faint">No workspace is trusted.</p>
      )}
      {list && list.length > 0 && (
        <ul
          aria-label="Trusted workspaces"
          className="mt-4 divide-y divide-line rounded-[10px] border border-line"
        >
          {list.map((w) => (
            <li key={w.workspace} className="flex items-center gap-3 px-4 py-2.5">
              <div className="min-w-0 flex-1">
                <div className="truncate text-[13.5px] font-medium">{folderName(w.workspace)}</div>
                <div className="truncate font-mono text-[11.5px] text-faint" title={w.workspace}>
                  {w.workspace}
                </div>
                {w.approvedHosts.length > 0 && (
                  <div className="text-[12px] text-muted">
                    Key sent to: {w.approvedHosts.join(", ")}
                  </div>
                )}
              </div>
              <Button
                variant="outline"
                size="sm"
                aria-label={`Forget ${folderName(w.workspace)}`}
                onClick={() => void forget(w.workspace)}
              >
                Forget
              </Button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

export function SettingsView({
  onForgotten,
}: {
  onForgotten: (opened: OpenedWorkspace | null) => void;
}) {
  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto w-full max-w-[720px] px-8 py-10">
        <h1 className="text-[26px] font-semibold tracking-tight">Settings</h1>

        <section className="border-b border-line py-7 last:border-b-0">
          <h2 className="text-[15px] font-semibold">Appearance</h2>
          <p className="mt-0.5 text-[13px] text-muted">
            Choose how the app looks. Your choice is remembered on this Mac.
          </p>
          <div className="mt-4 grid grid-cols-3 gap-3">
            <ThemeCard id="system" label="System" icon={<Monitor size={15} />} />
            <ThemeCard id="dark" label="Dark" icon={<Moon size={15} />} />
            <ThemeCard id="light" label="Light" icon={<Sun size={15} />} />
          </div>
        </section>

        <TrustedWorkspaces onForgotten={onForgotten} />
      </div>
    </div>
  );
}
