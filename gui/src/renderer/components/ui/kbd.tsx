import type { ReactNode } from "react";

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="inline-flex min-w-5 items-center justify-center rounded-[5px] border border-line-strong bg-s2 px-1.5 font-mono text-[11px] text-muted">
      {children}
    </kbd>
  );
}
