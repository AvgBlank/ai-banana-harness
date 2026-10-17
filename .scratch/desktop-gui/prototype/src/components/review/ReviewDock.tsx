import { ChevronDown, ChevronUp, FlaskConical, Monitor, Moon, Sun } from "lucide-react";
import { useState } from "react";
import { cn } from "@/lib/cn";
import { initialState, useApp, type AppState, type Theme } from "@/state/store";

type Preset = { label: string; patch: (s: AppState) => Partial<AppState> };

const BASE: Partial<AppState> = {
  dialog: null,
  modelOpen: false,
  workspaceMenuOpen: false,
  keyState: "present",
  sidecar: "ready",
  streaming: false,
  changesEmpty: false,
  trustedWorkspaces: ["ws-harness"],
  workspaceId: "ws-harness",
};

const PRESETS: Preset[] = [
  { label: "1 Home + composer", patch: () => ({ ...BASE, view: "home", sessionId: null, changesOpen: false }) },
  { label: "1b Workspace picker", patch: () => ({ ...BASE, view: "home", sessionId: null, changesOpen: false, workspaceMenuOpen: true }) },
  { label: "2 Conversation, streaming", patch: (s) => ({ ...BASE, view: "session", sessionId: "s1", changesOpen: false, streaming: true, streamRun: s.streamRun + 1 }) },
  { label: "2b Conversation, finished", patch: () => ({ ...BASE, view: "session", sessionId: "s1", changesOpen: false }) },
  { label: "2c Interrupted turn", patch: () => ({ ...BASE, view: "session", sessionId: "s3", changesOpen: false }) },
  { label: "3 Sidebar + rail", patch: () => ({ ...BASE, view: "session", sessionId: "s2", sidebarOpen: true, changesOpen: false }) },
  { label: "4 Changes (last turn)", patch: () => ({ ...BASE, view: "session", sessionId: "s1", changesOpen: true, changesScope: "turn" }) },
  { label: "4b Changes (binary/truncated)", patch: () => ({ ...BASE, view: "session", sessionId: "s1", changesOpen: true, changesScope: "workspace", selectedFile: "crates/harness-tui/src/app.rs" }) },
  { label: "4c Changes (empty)", patch: () => ({ ...BASE, view: "session", sessionId: "s2", changesOpen: true, changesEmpty: true }) },
  { label: "5 Approval dialog", patch: () => ({ ...BASE, view: "session", sessionId: "s1", changesOpen: false, dialog: "approval" }) },
  { label: "6 Provider / model", patch: () => ({ ...BASE, view: "home", sessionId: null, changesOpen: false, modelOpen: true }) },
  { label: "7 API-key onboarding", patch: () => ({ ...BASE, view: "home", sessionId: null, changesOpen: false, keyState: "missing" }) },
  { label: "8 Workspace trust", patch: () => ({ ...BASE, view: "home", sessionId: null, workspaceId: "ws-payments", changesOpen: false, dialog: "trust" }) },
  { label: "9 Restricted mode", patch: () => ({ ...BASE, view: "home", sessionId: null, workspaceId: "ws-payments", changesOpen: false }) },
  { label: "10 Sidecar restarting", patch: () => ({ ...BASE, view: "session", sessionId: "s1", changesOpen: false, sidecar: "crashed" }) },
  { label: "10b Sidecar failed", patch: () => ({ ...BASE, view: "session", sessionId: "s1", changesOpen: false, sidecar: "failed" }) },
  { label: "11 Settings", patch: () => ({ ...BASE, view: "settings", changesOpen: false }) },
  { label: "Quit while running", patch: () => ({ ...BASE, view: "session", sessionId: "s1", streaming: true, dialog: "quit" }) },
  { label: "External link", patch: () => ({ ...BASE, view: "session", sessionId: "s1", dialog: "link" }) },
];

export function ReviewDock() {
  const { state, set } = useApp();
  const [open, setOpen] = useState(true);

  return (
    <div
      className="fixed bottom-3 left-3 z-[70] w-[236px] rounded-[12px] border border-line-strong bg-s1/95 text-[12px] shadow-pop backdrop-blur"
      aria-label="Prototype controls"
    >
      <button onClick={() => setOpen(!open)} className="flex h-9 w-full cursor-pointer items-center gap-2 px-3 text-left font-medium text-muted hover:text-fg">
        <FlaskConical size={14} className="text-accent-text" />
        Prototype controls
        {open ? <ChevronDown size={14} className="ml-auto" /> : <ChevronUp size={14} className="ml-auto" />}
      </button>
      {open && (
        <div className="border-t border-line p-2">
          <div className="mb-2 flex gap-1">
            {(
              [
                ["system", <Monitor key="m" size={13} />],
                ["dark", <Moon key="d" size={13} />],
                ["light", <Sun key="l" size={13} />],
              ] as [Theme, React.ReactNode][]
            ).map(([id, icon]) => (
              <button
                key={id}
                aria-pressed={state.theme === id}
                onClick={() => set({ theme: id })}
                className={cn(
                  "flex h-7 flex-1 cursor-pointer items-center justify-center gap-1 rounded-md capitalize",
                  state.theme === id ? "bg-accent text-accent-fg" : "bg-s2 text-muted hover:text-fg",
                )}
              >
                {icon}
                {id}
              </button>
            ))}
          </div>
          <div className="max-h-[46vh] space-y-px overflow-y-auto">
            {PRESETS.map((p) => (
              <button
                key={p.label}
                onClick={() => set({ ...initialState, theme: state.theme, ...p.patch(state), streamRun: state.streamRun + 1 })}
                className="block h-7 w-full cursor-pointer truncate rounded px-2 text-left text-muted hover:bg-s2 hover:text-fg"
              >
                {p.label}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
