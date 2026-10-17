import { KeyOnboarding } from "@/components/dialogs/KeyOnboarding";
import { Composer } from "@/components/home/Composer";
import { WorkspacePicker } from "@/components/home/WorkspacePicker";
import { BananaMark } from "@/components/ui/banana-mark";
import { WORKSPACES } from "@/mock/data";
import { useApp } from "@/state/store";

const SUGGESTIONS = ["Explain how a turn is checkpointed", "Add a test for rename_session", "Find places that print to stdout"];

export function HomeScreen() {
  const { state, set } = useApp();
  const ws = WORKSPACES.find((w) => w.id === state.workspaceId) ?? WORKSPACES[0];
  const needsKey = state.keyState === "missing";

  return (
    <div className="mx-auto flex h-full w-full max-w-[820px] flex-col justify-center px-8 pb-10">
      <div className="mb-10 flex flex-col items-center text-center">
        <BananaMark size={64} />
        <h1 className="mt-6 text-[32px] font-medium leading-tight tracking-tight">
          What should we work on in{" "}
          <span className="underline decoration-accent-line decoration-2 underline-offset-[6px]">{ws.name}</span>?
        </h1>
        <p className="mt-2 text-[14px] text-muted">The agent works inside this workspace. Everything it changes can be undone.</p>
      </div>

      {needsKey && (
        <div className="mb-5">
          <KeyOnboarding />
        </div>
      )}

      <div>
        <WorkspacePicker />
        <Composer
          disabled={needsKey}
          disabledReason="Add an API key to start a session."
          onSend={() => set({ view: "session", sessionId: "s1", streaming: true, streamRun: state.streamRun + 1 })}
        />
      </div>

      {!needsKey && (
        <div className="mt-5 flex flex-wrap justify-center gap-2">
          {SUGGESTIONS.map((s) => (
            <button
              key={s}
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
