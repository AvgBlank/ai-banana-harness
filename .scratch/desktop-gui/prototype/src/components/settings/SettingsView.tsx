import { Check, KeyRound, Monitor, Moon, RefreshCw, Sun } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { cn } from "@/lib/cn";
import { APP_NAME, WORKSPACES } from "@/mock/data";
import { useApp, type Theme } from "@/state/store";

/** Fixed colours on purpose: each card previews its own theme regardless of the active one. */
const PREVIEWS: Record<Theme, { bg: string; s1: string; line: string; fg: string; accent: string; add: string }> = {
  dark: { bg: "#1a1914", s1: "#211f19", line: "#3a362c", fg: "#a8a28f", accent: "#ffd60a", add: "#2f8a52" },
  light: { bg: "#fbf6e4", s1: "#fffdf3", line: "#e2d8b3", fg: "#6b6448", accent: "#ffd60a", add: "#3a9a5b" },
  system: { bg: "linear-gradient(90deg,#1a1914 50%,#fbf6e4 50%)", s1: "", line: "#8a8368", fg: "#8a8368", accent: "#ffd60a", add: "#3a9a5b" },
};

function ThemeCard({ id, label, icon }: { id: Theme; label: string; icon: React.ReactNode }) {
  const { state, set } = useApp();
  const p = PREVIEWS[id];
  const active = state.theme === id;
  return (
    <button
      onClick={() => set({ theme: id })}
      aria-pressed={active}
      className={cn(
        "group cursor-pointer rounded-[14px] border p-2 text-left transition-colors",
        active ? "border-accent bg-accent-soft" : "border-line hover:border-line-strong hover:bg-s2",
      )}
    >
      <div className="h-[84px] overflow-hidden rounded-[9px] border" style={{ background: p.bg, borderColor: p.line }}>
        <div className="flex h-full">
          <div className="w-[22%] border-r p-1.5" style={{ borderColor: p.line, background: id === "system" ? "transparent" : p.s1 }}>
            <div className="mb-1 h-1.5 w-3/4 rounded-full" style={{ background: p.accent }} />
            <div className="mb-1 h-1.5 w-full rounded-full" style={{ background: p.line }} />
            <div className="h-1.5 w-2/3 rounded-full" style={{ background: p.line }} />
          </div>
          <div className="flex-1 space-y-1 p-2">
            <div className="h-1.5 w-4/5 rounded-full" style={{ background: p.line }} />
            <div className="h-1.5 w-3/5 rounded-full" style={{ background: p.line }} />
            <div className="mt-2 h-3 w-4/5 rounded-[3px]" style={{ background: p.add, opacity: 0.45 }} />
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

function Block({ title, hint, children }: { title: string; hint?: string; children: React.ReactNode }) {
  return (
    <section className="border-b border-line py-7 last:border-b-0">
      <h2 className="text-[15px] font-semibold">{title}</h2>
      {hint && <p className="mt-0.5 text-[13px] text-muted">{hint}</p>}
      <div className="mt-4">{children}</div>
    </section>
  );
}

export function SettingsView() {
  const { state, set } = useApp();
  const trusted = WORKSPACES.filter((w) => state.trustedWorkspaces.includes(w.id) && w.hasConfig);
  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto w-full max-w-[720px] px-8 py-10">
        <h1 className="text-[26px] font-semibold tracking-tight">Settings</h1>

        <Block title="Appearance" hint="Banana Harness follows your system by default. Your choice is remembered on this Mac.">
          <div className="grid grid-cols-3 gap-3">
            <ThemeCard id="system" label="System" icon={<Monitor size={15} />} />
            <ThemeCard id="dark" label="Dark" icon={<Moon size={15} />} />
            <ThemeCard id="light" label="Light" icon={<Sun size={15} />} />
          </div>
        </Block>

        <Block title="API key" hint="Read from the AI_API_KEY environment variable. It is never stored by the app.">
          <div className="flex items-center gap-3 rounded-[12px] border border-line bg-s1 px-4 py-3">
            <span className="grid h-9 w-9 place-items-center rounded-[9px] bg-s3 text-muted">
              <KeyRound size={17} />
            </span>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 text-[14px] font-medium">
                {state.keyState === "present" ? "Key found" : "No key found"}
                {state.keyState === "present" ? <Badge tone="ok">login shell</Badge> : <Badge tone="warn">missing</Badge>}
              </div>
              <div className="text-[12.5px] text-muted">The value is never shown here.</div>
            </div>
            <Button variant="outline" size="sm">
              <RefreshCw size={13} />
              Check again
            </Button>
          </div>
        </Block>

        <Block title="Trusted workspaces" hint="Workspaces whose .banana-harness/config.toml is allowed to run. Forgetting one puts it back in restricted mode.">
          {trusted.length === 0 ? (
            <p className="rounded-[12px] border border-dashed border-line px-4 py-5 text-[13px] text-muted">No trusted workspaces. Folders without a config file never need trust.</p>
          ) : (
            <ul className="divide-y divide-line rounded-[12px] border border-line bg-s1">
              {trusted.map((w) => (
                <li key={w.id} className="flex items-center gap-3 px-4 py-3">
                  <div className="min-w-0 flex-1">
                    <div className="text-[14px]">{w.name}</div>
                    <div className="truncate font-mono text-[11.5px] text-faint">{w.path}</div>
                  </div>
                  <Button variant="outline" size="sm" onClick={() => set({ trustedWorkspaces: state.trustedWorkspaces.filter((id) => id !== w.id) })}>
                    Forget
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </Block>

        <Block title="Keyboard shortcuts">
          <ul className="grid grid-cols-2 gap-x-8 gap-y-2.5 text-[13px]">
            {(
              [
                ["Send message", ["↵"]],
                ["New line", ["⇧", "↵"]],
                ["Stop the running turn", ["⌘", "."]],
                ["New session", ["⌘", "N"]],
                ["Toggle sidebar", ["⌘", "B"]],
                ["Toggle changes", ["⌘", "⇧", "D"]],
              ] as [string, string[]][]
            ).map(([label, keys]) => (
              <li key={label} className="flex items-center justify-between">
                <span className="text-muted">{label}</span>
                <span className="flex gap-1">
                  {keys.map((k) => (
                    <Kbd key={k}>{k}</Kbd>
                  ))}
                </span>
              </li>
            ))}
          </ul>
        </Block>

        <Block title="About">
          <dl className="grid grid-cols-[120px_1fr] gap-y-1.5 text-[13px]">
            <dt className="text-muted">App</dt>
            <dd>{APP_NAME} 0.1.0 (prototype)</dd>
            <dt className="text-muted">Protocol</dt>
            <dd>1</dd>
            <dt className="text-muted">Sidecar</dt>
            <dd className="font-mono text-[12px]">banana-harness serve --stdio</dd>
          </dl>
        </Block>
      </div>
    </div>
  );
}
