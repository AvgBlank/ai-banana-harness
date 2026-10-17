import { Check, ChevronDown, Cpu } from "lucide-react";
import { useEffect, useState } from "react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/cn";
import { useClient } from "@/state/client-context";
import { useApp } from "@/state/store";
import type { ProviderOption } from "../../../shared/protocol";

const LABELS: Record<string, string> = { deepseek: "DeepSeek", qwen: "Qwen" };

export function ModelPicker({
  provider,
  onPick,
}: {
  provider: string | null;
  onPick: (id: string) => void;
}) {
  const client = useClient();
  const { state, set } = useApp();
  const [options, setOptions] = useState<ProviderOption[]>([]);
  const [fallback, setFallback] = useState<string | null>(null);

  useEffect(() => {
    setOptions([]);
    if (!state.workspace) return;
    let cancelled = false;
    client
      .providers(state.workspace)
      .then((result) => {
        if (cancelled) return;
        setOptions(result.providers);
        setFallback(result.default);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client, state.workspace]);

  const activeId = provider ?? fallback;
  const active = options.find((p) => p.provider === activeId) ?? options[0];
  if (!active) {
    return (
      <button
        type="button"
        disabled
        aria-label="Provider and model: none until a workspace is open"
        className="flex h-8 items-center gap-1.5 rounded-md px-2 text-[13px] text-muted disabled:opacity-45"
      >
        <Cpu size={14} />
        <span className="font-medium text-fg">No provider</span>
        <ChevronDown size={13} />
      </button>
    );
  }
  const label = LABELS[active.provider] ?? active.provider;
  return (
    <Popover open={state.modelOpen} onOpenChange={(modelOpen) => set({ modelOpen })}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={`Provider and model: ${label} ${active.model}`}
          className="flex h-8 cursor-pointer items-center gap-1.5 rounded-md px-2 text-[13px] text-muted hover:bg-s3 hover:text-fg data-[state=open]:bg-s3 data-[state=open]:text-fg"
        >
          <Cpu size={14} />
          <span className="font-medium text-fg">{label}</span>
          <span className="hidden font-mono text-[12px] @sm:inline">{active.model}</span>
          <ChevronDown size={13} />
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="end" className="w-[340px]">
        <div className="px-2.5 pb-1.5 pt-1 text-[12px] font-medium text-muted">
          Provider for this session
        </div>
        {options.map((p) => (
          <button
            type="button"
            key={p.provider}
            onClick={() => {
              onPick(p.provider);
              set({ modelOpen: false });
            }}
            className={cn(
              "flex w-full cursor-pointer items-start gap-3 rounded-md px-2.5 py-2 text-left hover:bg-s2",
              p.provider === active.provider && "bg-s2",
            )}
          >
            <span className="mt-0.5 grid h-4 w-4 place-items-center text-accent-text">
              {p.provider === active.provider && <Check size={15} strokeWidth={2.5} />}
            </span>
            <span className="min-w-0 flex-1">
              <span className="flex items-baseline gap-2">
                <span className="text-[14px] font-medium text-fg">
                  {LABELS[p.provider] ?? p.provider}
                </span>
                <span className="font-mono text-[12px] text-muted">{p.model}</span>
              </span>
              <span className="mt-0.5 block text-[12px] text-faint">
                Your key is sent to {p.scheme === "https" ? "" : `${p.scheme}://`}
                {p.host}
              </span>
            </span>
          </button>
        ))}
        <div className="mt-1 border-t border-line px-2.5 pb-1 pt-2 text-[12px] text-faint">
          One model per provider in this version. A key belongs to one provider; if you get a 401,
          switch provider.
        </div>
      </PopoverContent>
    </Popover>
  );
}
