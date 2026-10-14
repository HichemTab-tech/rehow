import type { GuideSummary } from "@/api/payloads";
import { cn } from "@/lib/utils";

const STYLES: Record<
  GuideSummary["status"],
  { dot: string; text: string; label: string; hint: string }
> = {
  Tested: {
    dot: "bg-accent",
    text: "text-accent-text",
    label: "Tested",
    hint: "You confirmed these steps work.",
  },
  Draft: {
    dot: "border border-faint bg-transparent",
    text: "text-muted",
    label: "Draft",
    hint: "Not confirmed yet.",
  },
  Stale: {
    dot: "bg-warning",
    text: "text-warning",
    label: "Needs retest",
    hint: "Changed since it was last confirmed.",
  },
  Archived: {
    dot: "bg-faint",
    text: "text-faint",
    label: "Archived",
    hint: "Hidden from normal search.",
  },
};

export function statusLabel(status: GuideSummary["status"]) {
  return STYLES[status].label;
}

export function statusHint(status: GuideSummary["status"]) {
  return STYLES[status].hint;
}

export function StatusDot({
  status,
  className,
}: {
  status: GuideSummary["status"];
  className?: string;
}) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-block size-2 shrink-0 rounded-full",
        STYLES[status].dot,
        className,
      )}
    />
  );
}

export function StatusBadge({ status }: { status: GuideSummary["status"] }) {
  const style = STYLES[status];
  return (
    <span
      title={style.hint}
      className={cn(
        "inline-flex h-6 items-center gap-1.5 rounded-md border border-border px-2 text-xs font-medium",
        style.text,
      )}
    >
      <StatusDot status={status} />
      {style.label}
    </span>
  );
}
