import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type * as React from "react";
import { cn } from "../../lib/utils";

const buttonVariants = cva("button", {
  variants: {
    variant: {
      default: "",
      primary: "button-primary",
      danger: "button-danger",
    },
  },
  defaultVariants: {
    variant: "default",
  },
});

function Button({
  className,
  variant = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean;
  }) {
  const Component = asChild ? Slot : "button";
  return (
    <Component
      data-slot="button"
      data-variant={variant}
      className={cn(buttonVariants({ variant }), className)}
      {...props}
    />
  );
}

export { Button, buttonVariants };
