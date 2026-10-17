import { ArrowDown, TriangleAlert } from "lucide-react";
import { Fragment, type ReactNode, useEffect, useRef, useState } from "react";
import { Composer } from "@/components/home/Composer";
import type { TurnView } from "@/state/reduceTurn";
import type { LoadStatus } from "@/state/transcripts";

/** Renders `code` spans from backticks; a Markdown renderer arrives with the full transcript. */
function RichText({ text }: { text: string }) {
  // Keyed by where each part starts in the text, which is unique and stable.
  let offset = 0;
  const parts = text.split(/(`[^`]+`)/g).map((part) => {
    const key = offset;
    offset += part.length;
    return { key, part };
  });
  return (
    <>
      {parts.map(({ key, part }) =>
        part.startsWith("`") && part.length > 1 ? (
          <code
            key={key}
            className="rounded-[5px] bg-s3 px-1.5 py-0.5 font-mono text-[12.5px] text-fg"
          >
            {part.slice(1, -1)}
          </code>
        ) : (
          <Fragment key={key}>{part}</Fragment>
        ),
      )}
    </>
  );
}

function UserMessage({ children }: { children: ReactNode }) {
  return (
    <div className="flex justify-end">
      <div className="max-w-[78%] whitespace-pre-wrap rounded-[18px] rounded-br-md bg-s3 px-4 py-2.5 text-[14.5px] leading-relaxed">
        {children}
      </div>
    </div>
  );
}

const KEY_PROBLEM = /\b401\b|unauthori[sz]ed|invalid api key|incorrect api key/i;
const BOTTOM_SLACK_PX = 48;

export function SessionView({
  turns,
  load,
  running,
  stopping,
  provider,
  onPickProvider,
  onSend,
  onStop,
  keyMissing,
}: {
  turns: TurnView[];
  load: LoadStatus | undefined;
  running: boolean;
  stopping: boolean;
  provider: string | null;
  onPickProvider: (id: string) => void;
  onSend: (text: string) => Promise<void>;
  onStop: () => void;
  keyMissing: boolean;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const [stuck, setStuck] = useState(true);
  const stuckRef = useRef(true);

  const follow = () => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  };

  // New text keeps the view at the bottom while the user has not scrolled up.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the turns are the trigger
  useEffect(() => {
    if (stuckRef.current) follow();
  }, [turns]);

  const onScroll = () => {
    const el = scroller.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < BOTTOM_SLACK_PX;
    stuckRef.current = atBottom;
    setStuck(atBottom);
  };

  const jump = () => {
    stuckRef.current = true;
    setStuck(true);
    follow();
  };

  // Sending reattaches to the bottom.
  const send = async (text: string) => {
    stuckRef.current = true;
    setStuck(true);
    await onSend(text);
  };

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col">
      <div className="relative min-h-0 flex-1">
        <div ref={scroller} onScroll={onScroll} className="h-full overflow-y-auto">
          <div className="mx-auto flex w-full max-w-[780px] flex-col gap-4 px-6 pb-6 pt-8">
            {load === "loading" && turns.length === 0 && (
              <p className="text-[13px] text-faint">Loading…</p>
            )}
            {load === "error" && (
              <div
                role="alert"
                className="rounded-[12px] border border-warn/40 bg-warn-soft px-4 py-3 text-[13px] text-warn"
              >
                {keyMissing
                  ? "Add an API key to open this session."
                  : "This session could not be opened. Try again in a moment."}
              </div>
            )}
            {turns.map((turn, index) => (
              <TurnBlock key={turn.turnId ?? `pending-${index}`} turn={turn} />
            ))}
          </div>
        </div>
        {!stuck && (
          <button
            type="button"
            onClick={jump}
            className="absolute bottom-3 left-1/2 flex -translate-x-1/2 cursor-pointer items-center gap-1.5 rounded-full border border-line-strong bg-s2 px-3 py-1.5 text-[12px] text-fg shadow-pop hover:bg-s3"
          >
            <ArrowDown size={13} />
            Jump to latest
          </button>
        )}
      </div>
      <div className="mx-auto w-full max-w-[780px] px-6 pb-5">
        <Composer
          running={running}
          stopping={stopping}
          provider={provider}
          onPickProvider={onPickProvider}
          onSend={send}
          onStop={onStop}
          placeholder="Ask a follow-up. Enter to send, Shift+Enter for a new line."
        />
      </div>
    </div>
  );
}

function TurnBlock({ turn }: { turn: TurnView }) {
  const streaming = turn.status === "running";
  const last = turn.items.length - 1;
  return (
    <>
      <UserMessage>{turn.input}</UserMessage>
      {turn.items.map((item, i) => (
        <p
          key={`${item.threadId}-${item.messageId}`}
          className="whitespace-pre-wrap text-[14.5px] leading-relaxed text-fg/90"
        >
          <RichText text={item.text} />
          {streaming && i === last && !item.final && <span className="caret" aria-hidden />}
        </p>
      ))}
      {turn.status === "cancelled" && <p className="text-[12px] text-faint">Stopped.</p>}
      {turn.status === "interrupted" && (
        <div
          role="status"
          className="flex items-start gap-3 rounded-[12px] border border-warn/40 bg-warn-soft px-4 py-3 text-[13px]"
        >
          <TriangleAlert size={16} className="mt-0.5 shrink-0 text-warn" />
          <div>
            <div className="font-medium text-warn">This turn was interrupted</div>
            <div className="mt-0.5 text-muted">
              The sidecar stopped while it was running. Send a message to continue from the last
              saved context.
            </div>
          </div>
        </div>
      )}
      {turn.status === "error" && (
        <div
          role="alert"
          className="rounded-[12px] border border-danger/40 bg-danger-soft px-4 py-3 text-[13px]"
        >
          <div className="font-medium text-danger">The turn failed</div>
          {turn.error && <div className="mt-0.5 break-words text-muted">{turn.error}</div>}
          {turn.error && KEY_PROBLEM.test(turn.error) && (
            <div className="mt-1.5 text-muted">
              A key belongs to one provider. Switch provider with the picker below and send again.
            </div>
          )}
        </div>
      )}
    </>
  );
}
