import * as DialogPrimitive from "@radix-ui/react-dialog";
import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

export const Dialog = DialogPrimitive.Root;

/** A modal. A click outside never dismisses it, and Escape only does when the dialog gives it a safe way out. */
export function DialogContent({
  className,
  children,
  title,
  description,
  onOpenAutoFocus,
  onEscape,
}: {
  className?: string;
  children: ReactNode;
  title: string;
  description?: string;
  onOpenAutoFocus?: (e: Event) => void;
  /** Escape runs this; without it Escape does nothing, because the dialog needs an answer. */
  onEscape?: () => void;
}) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="fade-in fixed inset-0 z-50 bg-overlay" />
      <DialogPrimitive.Content
        onOpenAutoFocus={onOpenAutoFocus}
        onEscapeKeyDown={(e) => {
          e.preventDefault();
          onEscape?.();
        }}
        onInteractOutside={(e) => e.preventDefault()}
        className={cn(
          "pop-in fixed left-1/2 top-1/2 z-50 flex max-h-[88vh] w-[min(92vw,560px)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-line-strong bg-s1 shadow-pop",
          className,
        )}
      >
        <DialogPrimitive.Title className="sr-only">{title}</DialogPrimitive.Title>
        <DialogPrimitive.Description className="sr-only">
          {description ?? title}
        </DialogPrimitive.Description>
        {children}
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}
