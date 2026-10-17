import { Bot, History, Loader2, TriangleAlert } from "lucide-react";
import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import { KeyOnboarding } from "@/components/dialogs/KeyOnboarding";
import { Composer } from "@/components/home/Composer";
import { ToolCard } from "@/components/session/ToolCard";
import { TurnSummary } from "@/components/session/TurnSummary";
import { Badge } from "@/components/ui/badge";
import { APPROVAL, SESSIONS, STREAMING_TEXT, SUBAGENT, TOOL_CARDS, USER_PROMPT, type ToolCardData } from "@/mock/data";
import { useApp } from "@/state/store";

/** Renders `code` spans from backticks; the real transcript will use a Markdown renderer. */
function RichText({ text }: { text: string }) {
  return (
    <>
      {text.split(/(`[^`]+`)/g).map((part, i) =>
        part.startsWith("`") ? (
          <code key={i} className="rounded-[5px] bg-s3 px-1.5 py-0.5 font-mono text-[12.5px] text-fg">
            {part.slice(1, -1)}
          </code>
        ) : (
          <Fragment key={i}>{part}</Fragment>
        ),
      )}
    </>
  );
}

function UserMessage({ children }: { children: ReactNode }) {
  return (
    <div className="flex justify-end">
      <div className="max-w-[78%] rounded-[18px] rounded-br-md bg-s3 px-4 py-2.5 text-[14.5px] leading-relaxed">{children}</div>
    </div>
  );
}

function SubagentCard({ running }: { running: boolean }) {
  return (
    <div className="flex items-center gap-2.5 rounded-[12px] border border-line bg-s1 px-3 py-2.5">
      <span className="grid h-6 w-6 place-items-center rounded-md bg-peel-soft text-peel">
        <Bot size={14} />
      </span>
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px] font-medium">{SUBAGENT.title}</div>
        <div className="text-[12px] text-faint">Subagent · 3 tool calls</div>
      </div>
      {running ? <Loader2 size={15} className="animate-spin text-accent-text" /> : <Badge tone="ok">done</Badge>}
    </div>
  );
}

function Notice({ icon, children }: { icon: ReactNode; children: ReactNode }) {
  return (
    <div className="flex items-center gap-3 text-[12px] text-faint">
      <span className="h-px flex-1 bg-line" />
      <span className="flex items-center gap-1.5">
        {icon}
        {children}
      </span>
      <span className="h-px flex-1 bg-line" />
    </div>
  );
}

export function SessionView() {
  const { state, set } = useApp();
  const session = SESSIONS.find((s) => s.id === state.sessionId) ?? SESSIONS[0];
  const live = session.id === "s1";
  const [chars, setChars] = useState(STREAMING_TEXT.length);
  const scroller = useRef<HTMLDivElement>(null);
  const streaming = live && state.streaming;
  const waiting = state.dialog === "approval";
  const needsKey = state.keyState === "missing";

  useEffect(() => {
    if (!streaming) {
      setChars(STREAMING_TEXT.length);
      return;
    }
    setChars(0);
    const id = setInterval(() => {
      setChars((c) => {
        if (c >= STREAMING_TEXT.length) {
          clearInterval(id);
          set({ streaming: false });
          return c;
        }
        return c + 2;
      });
    }, 22);
    return () => clearInterval(id);
  }, [streaming, state.streamRun, set]);

  useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [chars, streaming, waiting]);

  const cards: ToolCardData[] = TOOL_CARDS.map((c) =>
    c.id === "t6" && streaming ? { ...c, state: "running", exitCode: undefined, detail: "running 41 tests\ntest checkpoint::undo_restores_the_workspace ... ok\ntest workspace_changes::lists_edits_and_new_files_with_patches ... ok" } : c,
  );

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col">
      <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex w-full max-w-[780px] flex-col gap-4 px-6 pb-6 pt-8">
          {live ? (
            <>
              <UserMessage>{USER_PROMPT}</UserMessage>
              <Notice icon={<History size={12} />}>Context compacted · 48k → 9k tokens</Notice>
              <p className="text-[14.5px] leading-relaxed text-fg/90">
                <RichText text="Let me find every caller of `workspace_changes` before moving it." />
              </p>
              {cards.slice(0, 5).map((c) => (
                <ToolCard key={c.id} card={c} defaultOpen={c.id === "t4"} />
              ))}
              <SubagentCard running={streaming} />
              <ToolCard card={cards[5]} defaultOpen={streaming} />
              {waiting && (
                <ToolCard card={{ id: "t7", name: "bash", summary: APPROVAL.command, state: "running", detail: "Waiting for your approval…" }} defaultOpen />
              )}
              <p className="text-[14.5px] leading-relaxed text-fg/90">
                <RichText text={STREAMING_TEXT.slice(0, chars)} />
                {streaming && <span className="caret" aria-hidden />}
              </p>
              {waiting && (
                <div className="flex items-center gap-2 rounded-[12px] border border-warn/40 bg-warn-soft px-3.5 py-2.5 text-[13px] text-warn">
                  <Loader2 size={14} className="animate-spin" />
                  The turn is paused until you answer the approval request.
                </div>
              )}
              {!streaming && !waiting && <TurnSummary />}
            </>
          ) : session.interrupted ? (
            <>
              <UserMessage>Write a README for the harness-bridge crate: protocol, methods and a wire example.</UserMessage>
              <p className="text-[14.5px] leading-relaxed text-fg/90">
                <RichText text="I'll read the protocol notes first, then draft the sections." />
              </p>
              <ToolCard card={{ id: "i1", name: "read_file", summary: ".scratch/desktop-gui/issues/03-sidecar-placement-and-protocol.md", state: "done", detail: "199 lines read" }} />
              <ToolCard card={{ id: "i2", name: "write_file", summary: "crates/harness-bridge/README.md", state: "error", detail: "Interrupted." }} />
              <div role="status" className="flex items-start gap-3 rounded-[12px] border border-warn/40 bg-warn-soft px-4 py-3 text-[13px]">
                <TriangleAlert size={16} className="mt-0.5 shrink-0 text-warn" />
                <div>
                  <div className="font-medium text-warn">This turn was interrupted</div>
                  <div className="mt-0.5 text-muted">The sidecar stopped while it was running. Files it already changed are still there, and you can undo them. Send a message to continue from the last saved context.</div>
                </div>
              </div>
            </>
          ) : (
            <>
              <UserMessage>{session.title}</UserMessage>
              <p className="text-[14.5px] leading-relaxed text-fg/90">
                <RichText text="Done. I made the change, ran the tests in `harness-core` and they pass. Let me know if you want it split into smaller commits." />
              </p>
            </>
          )}
        </div>
      </div>

      <div className="mx-auto w-full max-w-[780px] px-6 pb-5">
        {needsKey && (
          <div className="mb-3">
            <KeyOnboarding />
          </div>
        )}
        <Composer
          running={streaming || waiting}
          disabled={needsKey}
          disabledReason={needsKey ? "Add an API key to continue." : "A turn is running in this workspace."}
          placeholder="Reply to the agent"
          onStop={() => set({ streaming: false, dialog: null })}
        />
      </div>
    </div>
  );
}
