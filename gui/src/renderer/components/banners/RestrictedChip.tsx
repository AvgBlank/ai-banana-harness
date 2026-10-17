import { ShieldAlert } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/tooltip";

/** Shown while a workspace has a config the user has not trusted; it is not loaded. */
export function RestrictedChip({ onReview }: { onReview: () => void }) {
  return (
    <div className="no-drag flex items-center gap-1">
      <Tooltip label="Workspace config is not loaded">
        <Badge tone="peel" className="gap-1">
          <ShieldAlert size={12} />
          Restricted
        </Badge>
      </Tooltip>
      <Button variant="ghost" size="sm" onClick={onReview}>
        Review and trust…
      </Button>
    </div>
  );
}
