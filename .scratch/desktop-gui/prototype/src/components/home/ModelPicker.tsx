import { Check, ChevronDown, Cpu } from "lucide-react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/cn";
import { PROVIDERS } from "@/mock/data";
import { useApp } from "@/state/store";

export function ModelPicker() {
  const { state, set } = useApp();
  const active = PROVIDERS.find((p) => p.id === state.provider) ?? PROVIDERS[0];
  return (
    <Popover open={state.modelOpen} onOpenChange={(modelOpen) => set({ modelOpen })}>
      <PopoverTrigger asChild>
        <button
          aria-label={`Provider and model: ${active.label} ${active.model}`}
          className="flex h-8 cursor-pointer items-center gap-1.5 rounded-md px-2 text-[13px] text-muted hover:bg-s3 hover:text-fg data-[state=open]:bg-s3 data-[state=open]:text-fg"
        >
          <Cpu size={14} />
          <span className="font-medium text-fg">{active.label}</span>
          <span className="hidden font-mono text-[12px] @sm:inline">{active.model}</span>
          <ChevronDown size={13} />
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="end" className="w-[340px]">
        <div className="px-2.5 pb-1.5 pt-1 text-[12px] font-medium text-muted">Provider for this session</div>
        {PROVIDERS.map((p) => (
          <button
            key={p.id}
            onClick={() => set({ provider: p.id, modelOpen: false })}
            className={cn(
              "flex w-full cursor-pointer items-start gap-3 rounded-md px-2.5 py-2 text-left hover:bg-s2",
              p.id === state.provider && "bg-s2",
            )}
          >
            <span className="mt-0.5 grid h-4 w-4 place-items-center text-accent-text">{p.id === state.provider && <Check size={15} strokeWidth={2.5} />}</span>
            <span className="min-w-0 flex-1">
              <span className="flex items-baseline gap-2">
                <span className="text-[14px] font-medium text-fg">{p.label}</span>
                <span className="font-mono text-[12px] text-muted">{p.model}</span>
              </span>
              <span className="mt-0.5 block text-[12px] text-faint">Your key is sent to {p.host}</span>
            </span>
          </button>
        ))}
        <div className="mt-1 border-t border-line px-2.5 pb-1 pt-2 text-[12px] text-faint">
          One model per provider in this version. A key belongs to one provider; if you get a 401, switch provider.
        </div>
      </PopoverContent>
    </Popover>
  );
}
