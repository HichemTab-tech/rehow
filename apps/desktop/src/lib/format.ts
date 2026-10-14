const relative = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
const shortDate = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
});
const longDate = new Intl.DateTimeFormat(undefined, {
  year: "numeric",
  month: "long",
  day: "numeric",
});

/** "just now", "3 hours ago", "yesterday", then a short date. */
export function formatRelative(value: string, now = Date.now()) {
  const time = new Date(value).getTime();
  if (Number.isNaN(time)) return "";
  const seconds = Math.round((time - now) / 1000);
  const abs = Math.abs(seconds);
  if (abs < 60) return "just now";
  if (abs < 3600) return relative.format(Math.round(seconds / 60), "minute");
  if (abs < 86_400) return relative.format(Math.round(seconds / 3600), "hour");
  if (abs < 7 * 86_400)
    return relative.format(Math.round(seconds / 86_400), "day");
  return shortDate.format(time);
}

export function formatLongDate(value: string) {
  const time = new Date(value).getTime();
  return Number.isNaN(time) ? "" : longDate.format(time);
}

export function formatBytes(value: number | null) {
  if (value === null) return "";
  const format = new Intl.NumberFormat(undefined, {
    maximumFractionDigits: value < 1024 ? 0 : 1,
  });
  if (value < 1024) return `${format.format(value)} B`;
  if (value < 1_048_576) return `${format.format(value / 1024)} KB`;
  return `${format.format(value / 1_048_576)} MB`;
}

export function plural(count: number, one: string, many = `${one}s`) {
  return `${count.toLocaleString()} ${count === 1 ? one : many}`;
}

export const isMac =
  typeof navigator !== "undefined" && /Mac/.test(navigator.platform);
export const modKey = isMac ? "⌘" : "Ctrl";
