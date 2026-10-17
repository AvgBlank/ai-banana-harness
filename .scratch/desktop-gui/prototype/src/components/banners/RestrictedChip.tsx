import { ShieldAlert } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Tooltip } from "@/components/ui/tooltip";
import { useApp } from "@/state/store";

export function RestrictedChip({ compact = false }: { compact?: boolean }) {
  const { set } = useApp();
  return (
    <Tooltip label="Restricted mode: this workspace's config is not loaded. Click to review and trust.">
      <button aria-label="Restricted mode. Review and trust" onClick={() => set({ dialog: "trust" })} className="cursor-pointer">
        <Badge tone="peel">
          <ShieldAlert size={11} />
          {compact ? "Restricted" : "Restricted mode"}
        </Badge>
      </button>
    </Tooltip>
  );
}
