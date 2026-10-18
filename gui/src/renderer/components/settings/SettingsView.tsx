import { Check, KeyRound, Monitor, Moon, RefreshCw, Sun } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { cn } from "@/lib/cn";
import { folderName } from "@/lib/format";
import { useClient } from "@/state/client-context";
import { type Theme, useApp } from "@/state/store";
import type {
  AppInfo,
  CommandRunnerInfo,
  KeyStatus,
  OpenedWorkspace,
  TrustedWorkspace,
} from "../../../shared/api";

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

function Block({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return (
    <section className="border-b border-line py-7 last:border-b-0">
      <h2 className="text-[15px] font-semibold">{title}</h2>
      {hint && <p className="mt-0.5 text-[13px] text-muted">{hint}</p>}
      <div className="mt-4">{children}</div>
    </section>
  );
}

const KEY_SOURCES: Record<KeyStatus["source"], string> = {
  env: "environment",
  login_shell: "login shell",
  session: "this session only",
  none: "missing",
};

/** Whether a key exists and where it came from; the value never reaches the renderer. */
function ApiKey({
  status,
  onRecheck,
}: {
  status: KeyStatus | null;
  onRecheck: () => Promise<void>;
}) {
  const [checking, setChecking] = useState(false);
  const present = status?.present ?? false;
  return (
    <Block
      title="API key"
      hint="Read from the AI_API_KEY environment variable. It is never stored by the app."
    >
      <div className="flex items-center gap-3 rounded-[12px] border border-line bg-s1 px-4 py-3">
        <span className="grid h-9 w-9 place-items-center rounded-[9px] bg-s3 text-muted">
          <KeyRound size={17} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-[14px] font-medium">
            {status === null ? "Checking…" : present ? "Key found" : "No key found"}
            {status && <Badge tone={present ? "ok" : "warn"}>{KEY_SOURCES[status.source]}</Badge>}
          </div>
          <div className="text-[12.5px] text-muted">The value is never shown here.</div>
        </div>
        <Button
          variant="outline"
          size="sm"
          disabled={checking}
          onClick={async () => {
            setChecking(true);
            await onRecheck().catch(() => undefined);
            setChecking(false);
          }}
        >
          <RefreshCw size={13} />
          Check again
        </Button>
      </div>
    </Block>
  );
}

/** The keys that exist; a new one is added here, in the code and in the README together. */
const SHORTCUTS: Array<[string, string[]]> = [
  ["Send message", ["↵"]],
  ["New line", ["⇧", "↵"]],
  ["Close a dialog that can be dismissed", ["Esc"]],
];

function Shortcuts() {
  return (
    <Block title="Keyboard shortcuts">
      <ul className="grid grid-cols-2 gap-x-8 gap-y-2.5 text-[13px]">
        {SHORTCUTS.map(([label, keys]) => (
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
  );
}

function About() {
  const client = useClient();
  const [info, setInfo] = useState<AppInfo | null>(null);
  useEffect(() => {
    client.appInfo().then(setInfo, () => undefined);
  }, [client]);
  return (
    <Block title="About">
      <dl className="grid grid-cols-[120px_1fr] gap-y-1.5 text-[13px]">
        <dt className="text-muted">App</dt>
        <dd>{info ? `${info.name} ${info.version}` : "…"}</dd>
        <dt className="text-muted">Protocol</dt>
        <dd>{info?.protocol ?? "…"}</dd>
        <dt className="text-muted">Sidecar</dt>
        <dd className="font-mono text-[12px]">serve --stdio</dd>
      </dl>
    </Block>
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
    <Block
      title="Trusted workspaces"
      hint="Folders whose config you trusted. Trust is tied to the file's content: a change asks again. Forgetting one puts it back in restricted mode."
    >
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
    </Block>
  );
}

/** Read-only: where commands run is set in the user's own config file, never from the app. */
function CommandExecution({ runner }: { runner: CommandRunnerInfo | null }) {
  const container = runner?.kind === "container";
  return (
    <Block
      title="Command execution"
      hint="Where the agent's shell commands run. File tools and the app itself always run on this Mac."
    >
      <div className="rounded-[12px] border border-line bg-s1 p-4 text-[13px]">
        {runner === null ? (
          <p className="text-muted">Open a workspace to see how it runs commands.</p>
        ) : (
          <>
            <p className="flex items-center gap-2 font-medium">
              {container ? "Container" : "This Mac"}
              {container && runner.problem && <Badge>Unavailable</Badge>}
            </p>
            {container ? (
              <dl className="mt-2 grid grid-cols-[88px_1fr] gap-y-1 text-muted">
                <dt>Image</dt>
                <dd className="font-mono">{runner.image}</dd>
                <dt>Network</dt>
                <dd>{runner.network === "bridge" ? "On (bridge)" : "Off"}</dd>
              </dl>
            ) : (
              <p className="mt-1 text-muted">
                Commands run directly on this Mac, as you. Containers are off.
              </p>
            )}
            {runner.problem && (
              <p role="alert" className="mt-3 text-danger">
                {runner.problem}
              </p>
            )}
          </>
        )}
        <p className="mt-3 text-faint">
          To change this, edit <span className="font-mono">[tools.bash]</span> in your user config
          file. A workspace's own config cannot change it.
        </p>
      </div>
    </Block>
  );
}

export function SettingsView({
  keyStatus,
  runner,
  onRecheckKey,
  onForgotten,
}: {
  keyStatus: KeyStatus | null;
  runner: CommandRunnerInfo | null;
  onRecheckKey: () => Promise<void>;
  onForgotten: (opened: OpenedWorkspace | null) => void;
}) {
  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto w-full max-w-[720px] px-8 py-10">
        <h1 className="text-[26px] font-semibold tracking-tight">Settings</h1>

        <Block
          title="Appearance"
          hint="Choose how the app looks. Your choice is remembered on this Mac."
        >
          <div className="grid grid-cols-3 gap-3">
            <ThemeCard id="system" label="System" icon={<Monitor size={15} />} />
            <ThemeCard id="dark" label="Dark" icon={<Moon size={15} />} />
            <ThemeCard id="light" label="Light" icon={<Sun size={15} />} />
          </div>
        </Block>

        <ApiKey status={keyStatus} onRecheck={onRecheckKey} />

        <CommandExecution runner={runner} />

        <TrustedWorkspaces onForgotten={onForgotten} />

        <Shortcuts />

        <About />
      </div>
    </div>
  );
}
