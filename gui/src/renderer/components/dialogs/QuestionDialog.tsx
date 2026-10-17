import { MessageCircleQuestion } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import type { PendingAction } from "@/state/reduceTurn";

type Question = Extract<PendingAction, { kind: "question" }>;

/** The agent's question: its options as buttons, or a free-text answer. */
export function QuestionDialog({
  action,
  onAnswer,
}: {
  action: Question | undefined;
  onAnswer: (action: Question, text: string) => void;
}) {
  return (
    <Dialog open={action !== undefined}>
      {action && <QuestionBody key={action.actionId} action={action} onAnswer={onAnswer} />}
    </Dialog>
  );
}

function QuestionBody({
  action,
  onAnswer,
}: {
  action: Question;
  onAnswer: (a: Question, text: string) => void;
}) {
  const [text, setText] = useState("");
  const submit = () => {
    const answer = text.trim();
    if (answer) onAnswer(action, answer);
  };
  return (
    <DialogContent title="The agent has a question" description={action.question}>
      <div className="px-6 pb-4 pt-6">
        <div className="flex items-center gap-3">
          <span className="grid h-10 w-10 place-items-center rounded-[10px] bg-accent-soft text-accent-text">
            <MessageCircleQuestion size={20} />
          </span>
          <div>
            <h2 className="text-[17px] font-semibold">A question for you</h2>
            <p className="text-[13px] text-muted">The turn is paused until you answer.</p>
          </div>
        </div>
        <p className="mt-5 whitespace-pre-wrap text-[14.5px] leading-relaxed">{action.question}</p>
        {action.options.length > 0 && (
          <div className="mt-4 flex flex-col gap-2">
            {action.options.map((option) => (
              <Button
                key={option}
                variant="outline"
                className="justify-start"
                onClick={() => onAnswer(action, option)}
              >
                {option}
              </Button>
            ))}
          </div>
        )}
        <label className="mt-4 block text-[12px] text-muted" htmlFor="question-answer">
          {action.options.length > 0 ? "Or answer in your own words" : "Your answer"}
        </label>
        <div className="mt-1.5 flex gap-2">
          <input
            id="question-answer"
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") submit();
            }}
            autoComplete="off"
            className="h-9 min-w-0 flex-1 rounded-md border border-line-strong bg-s2 px-3 text-[14px] text-fg focus:border-accent-line focus:outline-none"
          />
          <Button variant="primary" disabled={!text.trim()} onClick={submit}>
            Send answer
          </Button>
        </div>
      </div>
    </DialogContent>
  );
}
