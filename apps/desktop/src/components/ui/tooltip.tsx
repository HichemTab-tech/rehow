import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import type * as React from "react";

export const TooltipProvider = TooltipPrimitive.Provider;

export function Tooltip({
  label,
  shortcut,
  side = "bottom",
  children,
}: {
  label: string;
  shortcut?: string;
  side?: "top" | "bottom" | "left" | "right";
  children: React.ReactNode;
}) {
  return (
    <TooltipPrimitive.Root>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          side={side}
          sideOffset={6}
          className="z-50 flex items-center gap-2 rounded-md bg-text px-2 py-1 text-xs font-medium text-bg shadow-menu data-[state=delayed-open]:animate-[fade-in_100ms_ease-out]"
        >
          {label}
          {shortcut ? <span className="opacity-60">{shortcut}</span> : null}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}
