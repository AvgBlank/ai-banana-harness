import { KeyOnboarding } from "@/components/dialogs/KeyOnboarding";
import { Composer } from "@/components/home/Composer";
import { WorkspacePicker } from "@/components/home/WorkspacePicker";
import { BananaMark } from "@/components/ui/banana-mark";
import { folderName } from "@/lib/format";
import { useApp } from "@/state/store";

const SUGGESTIONS = [
  "Explain how a turn is checkpointed",
  "Add a test for rename_session",
  "Find places that print to stdout",
];

export function HomeScreen({
  recent,
  canOpen,
  needsKey,
  appName,
  onRecheckKey,
  onUseKey,
  provider,
  onPickProvider,
  onOpenFolder,
  onOpenRecent,
}: {
  recent: string[];
  canOpen: boolean;
  needsKey: boolean;
  appName: string;
  onRecheckKey: () => Promise<void>;
  onUseKey: (key: string) => Promise<void>;
  provider: string | null;
  onPickProvider: (id: string) => void;
  onOpenFolder: () => void;
  onOpenRecent: (path: string) => Promise<void>;
}) {
  const { state, set } = useApp();
  return (
    <div className="mx-auto flex h-full w-full max-w-[820px] flex-col justify-center px-8 pb-10">
      <div className="mb-10 flex flex-col items-center text-center">
        <BananaMark size={64} />
        <h1 className="mt-6 text-[32px] font-medium leading-tight tracking-tight">
          What should we work on
          {state.workspace && (
            <>
              {" in "}
              <span className="underline decoration-accent-line decoration-2 underline-offset-[6px]">
                {folderName(state.workspace)}
              </span>
            </>
          )}
          ?
        </h1>
        <p className="mt-2 text-[14px] text-muted">
          The agent works inside this workspace. Everything it changes can be undone.
        </p>
      </div>

      {needsKey && (
        <div className="mb-5">
          <KeyOnboarding appName={appName} onRecheck={onRecheckKey} onUseKey={onUseKey} />
        </div>
      )}

      <div>
        <WorkspacePicker
          recent={recent}
          canOpen={canOpen}
          onOpenFolder={onOpenFolder}
          onOpenRecent={onOpenRecent}
        />
        <Composer
          disabled
          disabledReason={
            needsKey
              ? "Add an API key to start a session."
              : "Starting a session is not available yet."
          }
          provider={provider}
          onPickProvider={onPickProvider}
        />
      </div>

      {!needsKey && (
        <div className="mt-5 flex flex-wrap justify-center gap-2">
          {SUGGESTIONS.map((s) => (
            <button
              type="button"
              key={s}
              onClick={() => set({ draft: s })}
              className="cursor-pointer rounded-full border border-line px-3.5 py-1.5 text-[13px] text-muted transition-colors hover:border-line-strong hover:bg-s1 hover:text-fg"
            >
              {s}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
