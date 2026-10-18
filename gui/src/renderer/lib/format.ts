/** The last segment of a path: a workspace is shown by its folder name. */
export function folderName(path: string): string {
  const trimmed = path.replace(/[/\\]+$/, "");
  return trimmed.slice(trimmed.search(/[^/\\]*$/)) || path;
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** "now", "5m", "2h", "yesterday", "3d", "1w": how long ago `iso` was, as the sidebar shows it. */
export function relativeTime(iso: string, now: number): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "";
  const age = Math.max(0, now - then);
  if (age < MINUTE) return "now";
  if (age < HOUR) return `${Math.floor(age / MINUTE)}m`;
  if (age < DAY) return `${Math.floor(age / HOUR)}h`;
  if (age < 2 * DAY) return "yesterday";
  if (age < 7 * DAY) return `${Math.floor(age / DAY)}d`;
  return `${Math.floor(age / (7 * DAY))}w`;
}

/** The first character of a display name, upper-cased, for the avatar; `?` for none. */
export function initialOf(name: string | null | undefined): string {
  const first = Array.from((name ?? "").trim())[0];
  return first ? first.toUpperCase() : "?";
}
