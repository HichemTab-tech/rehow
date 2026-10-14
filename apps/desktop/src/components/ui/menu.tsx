import * as ContextMenuPrimitive from "@radix-ui/react-context-menu";
import * as DropdownPrimitive from "@radix-ui/react-dropdown-menu";
import type * as React from "react";
import { cn } from "@/lib/utils";

const contentClass =
  "z-50 min-w-44 overflow-hidden rounded-lg border border-border bg-raised p-1 text-sm text-text shadow-menu outline-none data-[state=open]:animate-[menu-in_100ms_ease-out]";
const itemClass =
  "relative flex h-7 cursor-default select-none items-center gap-2 rounded-md px-2 outline-none data-[disabled]:pointer-events-none data-[highlighted]:bg-hover data-[disabled]:opacity-45 [&_svg]:size-4 [&_svg]:text-muted";

export const DropdownMenu = DropdownPrimitive.Root;
export const DropdownMenuTrigger = DropdownPrimitive.Trigger;

export function DropdownMenuContent({
  className,
  sideOffset = 4,
  ...props
}: React.ComponentProps<typeof DropdownPrimitive.Content>) {
  return (
    <DropdownPrimitive.Portal>
      <DropdownPrimitive.Content
        sideOffset={sideOffset}
        className={cn(contentClass, className)}
        {...props}
      />
    </DropdownPrimitive.Portal>
  );
}

export function DropdownMenuItem({
  className,
  destructive,
  shortcut,
  children,
  ...props
}: React.ComponentProps<typeof DropdownPrimitive.Item> & {
  destructive?: boolean;
  shortcut?: string;
}) {
  return (
    <DropdownPrimitive.Item
      className={cn(
        itemClass,
        destructive &&
          "text-danger data-[highlighted]:bg-danger-soft [&_svg]:text-danger",
        className,
      )}
      {...props}
    >
      {children}
      {shortcut ? (
        <span className="ml-auto pl-4 text-xs text-faint">{shortcut}</span>
      ) : null}
    </DropdownPrimitive.Item>
  );
}

export function DropdownMenuSeparator() {
  return <DropdownPrimitive.Separator className="-mx-1 my-1 h-px bg-border" />;
}

export function DropdownMenuLabel({ children }: { children: React.ReactNode }) {
  return (
    <DropdownPrimitive.Label className="px-2 pt-1.5 pb-1 text-2xs font-medium text-faint uppercase tracking-wide">
      {children}
    </DropdownPrimitive.Label>
  );
}

export const DropdownMenuRadioGroup = DropdownPrimitive.RadioGroup;

export function DropdownMenuRadioItem({
  className,
  children,
  ...props
}: React.ComponentProps<typeof DropdownPrimitive.RadioItem>) {
  return (
    <DropdownPrimitive.RadioItem
      className={cn(itemClass, "pl-7", className)}
      {...props}
    >
      <DropdownPrimitive.ItemIndicator className="absolute left-2 inline-flex">
        <span className="size-1.5 rounded-full bg-accent" />
      </DropdownPrimitive.ItemIndicator>
      {children}
    </DropdownPrimitive.RadioItem>
  );
}

export const ContextMenu = ContextMenuPrimitive.Root;
export const ContextMenuTrigger = ContextMenuPrimitive.Trigger;

export function ContextMenuContent({
  className,
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.Content>) {
  return (
    <ContextMenuPrimitive.Portal>
      <ContextMenuPrimitive.Content
        className={cn(contentClass, className)}
        {...props}
      />
    </ContextMenuPrimitive.Portal>
  );
}

export function ContextMenuItem({
  className,
  destructive,
  ...props
}: React.ComponentProps<typeof ContextMenuPrimitive.Item> & {
  destructive?: boolean;
}) {
  return (
    <ContextMenuPrimitive.Item
      className={cn(
        itemClass,
        destructive &&
          "text-danger data-[highlighted]:bg-danger-soft [&_svg]:text-danger",
        className,
      )}
      {...props}
    />
  );
}

export function ContextMenuSeparator() {
  return (
    <ContextMenuPrimitive.Separator className="-mx-1 my-1 h-px bg-border" />
  );
}
