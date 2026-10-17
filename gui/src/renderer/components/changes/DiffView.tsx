import { useMemo } from "react";
import { cn } from "@/lib/cn";
import { parseDiff } from "@/state/diff";

/** More rows than this are windowed with `content-visibility`. */
export const WINDOW_AFTER_ROWS = 300;

export function DiffView({
  patch,
  truncated,
  className,
}: {
  patch: string;
  truncated?: boolean;
  className?: string;
}) {
  const { lines } = useMemo(() => parseDiff(patch), [patch]);
  // Long diffs let the browser skip the rows that are off screen.
  const windowed = lines.length > WINDOW_AFTER_ROWS;
  return (
    <section
      className={cn("overflow-x-auto font-mono text-[12.5px] leading-[1.65]", className)}
      aria-label="Diff"
    >
      {lines.map((l, i) => (
        <div
          // biome-ignore lint/suspicious/noArrayIndexKey: diff rows have no identity beyond their position
          key={i}
          className={cn(
            "grid min-w-max grid-cols-[44px_44px_3px_1fr]",
            windowed && "[contain-intrinsic-size:auto_21px] [content-visibility:auto]",
            l.kind === "add" && "bg-add-bg",
            l.kind === "del" && "bg-del-bg",
            l.kind === "hunk" && "bg-hunk text-muted",
          )}
        >
          <span className="select-none pr-2 text-right text-faint">
            {l.kind === "hunk" || l.kind === "note" ? "" : (l.oldNo ?? "")}
          </span>
          <span className="select-none pr-2 text-right text-faint">
            {l.kind === "hunk" || l.kind === "note" ? "" : (l.newNo ?? "")}
          </span>
          <span
            className={cn(l.kind === "add" && "bg-add-gutter", l.kind === "del" && "bg-del-gutter")}
          />
          <span
            className={cn(
              "whitespace-pre pl-3 pr-4",
              l.kind === "add" && "text-add-fg",
              l.kind === "del" && "text-del-fg",
              l.kind === "ctx" && "text-fg/85",
              l.kind === "note" && "italic text-faint",
            )}
          >
            <span className="mr-2 inline-block w-2 select-none text-faint">
              {l.kind === "add" ? "+" : l.kind === "del" ? "−" : ""}
            </span>
            {l.text || " "}
          </span>
        </div>
      ))}
      {truncated && (
        <div className="border-t border-line bg-warn-soft px-4 py-2 font-sans text-[12px] text-warn">
          Patch truncated at 256 KiB. Open the file to see the rest.
        </div>
      )}
    </section>
  );
}
