import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";
import { cn } from "@/lib/cn";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-[13px] font-medium transition-colors disabled:pointer-events-none disabled:opacity-45 cursor-pointer select-none",
  {
    variants: {
      variant: {
        primary: "bg-accent text-accent-fg hover:bg-accent-hover disabled:bg-s3 disabled:text-faint disabled:opacity-100",
        secondary: "bg-s3 text-fg hover:bg-line",
        outline: "border border-line-strong bg-transparent text-fg hover:bg-s2",
        ghost: "bg-transparent text-muted hover:bg-s2 hover:text-fg",
        danger: "bg-danger text-white hover:opacity-90",
        dangerOutline: "border border-danger/50 text-danger hover:bg-danger-soft",
      },
      size: {
        sm: "h-7 px-2.5",
        md: "h-8 px-3.5",
        lg: "h-10 px-5 text-sm",
        icon: "h-8 w-8",
        iconSm: "h-7 w-7",
      },
    },
    defaultVariants: { variant: "secondary", size: "md" },
  },
);

export type ButtonProps = ComponentProps<"button"> & VariantProps<typeof buttonVariants>;

export function Button({ className, variant, size, ...props }: ButtonProps) {
  return <button className={cn(buttonVariants({ variant, size }), className)} {...props} />;
}
