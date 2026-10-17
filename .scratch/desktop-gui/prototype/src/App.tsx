import { CrashBanner } from "@/components/banners/CrashBanner";
import { ChangesPanel } from "@/components/changes/ChangesPanel";
import { ApprovalDialog } from "@/components/dialogs/ApprovalDialog";
import { LinkDialog, QuitDialog } from "@/components/dialogs/SmallDialogs";
import { TrustDialog } from "@/components/dialogs/TrustDialog";
import { HomeScreen } from "@/components/home/HomeScreen";
import { ReviewDock } from "@/components/review/ReviewDock";
import { SessionView } from "@/components/session/SessionView";
import { SettingsView } from "@/components/settings/SettingsView";
import { IconRail } from "@/components/shell/IconRail";
import { Sidebar } from "@/components/shell/Sidebar";
import { TitleBar } from "@/components/shell/TitleBar";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useEffect, useState } from "react";
import { StateProvider, useApp } from "@/state/store";

function useNarrow() {
  const query = "(max-width: 1279px)";
  const [narrow, setNarrow] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const mq = window.matchMedia(query);
    const on = () => setNarrow(mq.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  return narrow;
}

function Shell() {
  const { state } = useApp();
  const narrow = useNarrow();
  const changesShown = state.changesOpen && state.view !== "settings";
  const sidebarShown = state.sidebarOpen && !(narrow && changesShown);
  return (
    <div className="flex h-full flex-col">
      <TitleBar />
      <CrashBanner />
      <div className="flex min-h-0 flex-1">
        <IconRail />
        {sidebarShown && <Sidebar />}
        <main className="min-w-0 flex-1 bg-bg">
          {state.view === "home" && <HomeScreen />}
          {state.view === "session" && <SessionView />}
          {state.view === "settings" && <SettingsView />}
        </main>
        {changesShown && <ChangesPanel empty={state.changesEmpty} />}
      </div>
      <ApprovalDialog />
      <TrustDialog />
      <QuitDialog />
      <LinkDialog />
      <ReviewDock />
    </div>
  );
}

export default function App() {
  return (
    <StateProvider>
      <TooltipProvider>
        <Shell />
      </TooltipProvider>
    </StateProvider>
  );
}
