import { Eye, EyeOff, KeyRound, RefreshCw } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";

export function KeyOnboarding({
  appName,
  onRecheck,
  onUseKey,
}: {
  appName: string;
  onRecheck: () => Promise<void>;
  onUseKey: (key: string) => Promise<void>;
}) {
  const [value, setValue] = useState("");
  const [show, setShow] = useState(false);
  const [checking, setChecking] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const check = async () => {
    setChecking(true);
    setMessage(null);
    try {
      await onRecheck();
    } catch {
      setMessage("Could not check just now. Try again.");
    } finally {
      setChecking(false);
    }
  };

  const use = async () => {
    const key = value.trim();
    if (!key) return;
    // The field is emptied at once: the key is not kept in the page after it is handed over.
    setValue("");
    setShow(false);
    setMessage(null);
    try {
      await onUseKey(key);
    } catch {
      setMessage("The key could not be used. Try again.");
    }
  };

  return (
    <section
      aria-labelledby="key-title"
      className="rounded-[18px] border border-line-strong bg-s1 p-5"
    >
      <div className="flex items-start gap-3.5">
        <span className="grid h-10 w-10 shrink-0 place-items-center rounded-[10px] bg-accent-soft text-accent-text">
          <KeyRound size={19} />
        </span>
        <div className="min-w-0 flex-1">
          <h2 id="key-title" className="text-[16px] font-semibold">
            No API key found
          </h2>
          <p className="mt-1 text-[13.5px] leading-relaxed text-muted">
            {appName} reads your key only from the{" "}
            <code className="rounded bg-s3 px-1.5 py-0.5 font-mono text-[12px] text-fg">
              AI_API_KEY
            </code>{" "}
            environment variable. It checked this app's environment and your login shell and found
            nothing. You can still browse past sessions.
          </p>

          <ol className="mt-3.5 space-y-2 text-[13.5px] text-muted">
            <li className="flex gap-2.5">
              <span className="mt-px grid h-5 w-5 shrink-0 place-items-center rounded-full bg-s3 text-[11px] font-medium text-fg">
                1
              </span>
              <span>
                Add{" "}
                <code className="rounded bg-s3 px-1.5 py-0.5 font-mono text-[12px] text-fg">
                  export AI_API_KEY=…
                </code>{" "}
                to your shell profile (for example{" "}
                <code className="font-mono text-[12px]">~/.zshrc</code>
                ), then check again.
              </span>
            </li>
            <li className="flex gap-2.5">
              <span className="mt-px grid h-5 w-5 shrink-0 place-items-center rounded-full bg-s3 text-[11px] font-medium text-fg">
                2
              </span>
              <span>Or paste a key below to use for this session only.</span>
            </li>
          </ol>

          <div className="mt-4 flex items-center gap-2">
            <div className="relative flex-1">
              <input
                type={show ? "text" : "password"}
                value={value}
                onChange={(e) => setValue(e.target.value)}
                autoComplete="off"
                spellCheck={false}
                aria-label="API key for this session"
                placeholder="Paste a DeepSeek or Qwen key"
                className="h-9 w-full rounded-md border border-line-strong bg-s2 pl-3 pr-9 font-mono text-[13px] text-fg placeholder:font-sans placeholder:text-faint focus:border-accent-line focus:outline-none"
              />
              <button
                type="button"
                aria-label={show ? "Hide key" : "Show key"}
                onClick={() => setShow(!show)}
                className="absolute right-1.5 top-1.5 grid h-6 w-6 cursor-pointer place-items-center rounded text-muted hover:text-fg"
              >
                {show ? <EyeOff size={15} /> : <Eye size={15} />}
              </button>
            </div>
            <Button variant="primary" disabled={!value.trim()} onClick={() => void use()}>
              Use for this session
            </Button>
          </div>
          <p className="mt-2 text-[12px] text-faint">
            Kept in memory until you quit. Never saved to disk, settings or logs.
          </p>
          {message && (
            <p role="alert" className="mt-2 text-[12px] text-danger">
              {message}
            </p>
          )}

          <div className="mt-4 flex items-center gap-2 border-t border-line pt-3.5">
            <Button variant="outline" disabled={checking} onClick={() => void check()}>
              <RefreshCw size={14} className={checking ? "animate-spin" : ""} />
              {checking ? "Checking…" : "Check again"}
            </Button>
            <span className="text-[12px] text-faint">
              Re-reads your environment and login shell.
            </span>
          </div>
        </div>
      </div>
    </section>
  );
}
