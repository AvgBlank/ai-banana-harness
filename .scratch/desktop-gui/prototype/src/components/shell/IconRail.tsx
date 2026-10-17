import { FileDiff, FolderTree, House, Settings } from "lucide-react";
import type { ReactNode } from "react";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/cn";
import { useApp } from "@/state/store";

function RailButton({ label, active, onClick, children }: { label: string; active?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <Tooltip label={label}>
      <button
        aria-label={label}
        aria-current={active ? "page" : undefined}
        onClick={onClick}
        className={cn(
          "relative grid h-10 w-10 cursor-pointer place-items-center rounded-[10px] text-muted transition-colors hover:bg-s2 hover:text-fg",
          active && "bg-accent-soft text-accent-text hover:bg-accent-soft hover:text-accent-text",
        )}
      >
        {active && <span className="absolute -left-[7px] top-2.5 h-5 w-[3px] rounded-r-full bg-accent" />}
        {children}
      </button>
    </Tooltip>
  );
}

export function IconRail() {
  const { state, set } = useApp();
  return (
    <nav
      aria-label="Primary"
      className="flex shrink-0 flex-col items-center gap-1.5 border-r border-line bg-bg py-3"
      style={{ width: "var(--rail-w)" }}
    >
      <RailButton label="Home" active={state.view === "home"} onClick={() => set({ view: "home", sessionId: null })}>
        <House size={19} />
      </RailButton>
      <RailButton label="Sessions" active={state.view === "session"} onClick={() => set({ sidebarOpen: true, view: "session", sessionId: state.sessionId ?? "s1" })}>
        <FolderTree size={19} />
      </RailButton>
      <RailButton label="Changes" active={state.changesOpen} onClick={() => set({ changesOpen: !state.changesOpen })}>
        <FileDiff size={19} />
      </RailButton>
      <div className="mt-auto flex flex-col items-center gap-1.5">
        <RailButton label="Settings" active={state.view === "settings"} onClick={() => set({ view: "settings" })}>
          <Settings size={19} />
        </RailButton>
      </div>
    </nav>
  );
}
