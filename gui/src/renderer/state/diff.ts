export interface DiffLine {
  kind: "ctx" | "add" | "del" | "hunk" | "note";
  oldNo?: number;
  newNo?: number;
  text: string;
}

export interface ParsedDiff {
  lines: DiffLine[];
  added: number;
  removed: number;
}

/**
 * A unified diff as rows with line numbers. Everything before the first `@@` (the
 * `--- before` and `+++ after` headers) is skipped, and `\\ No newline at end of file` is a
 * note that neither numbers nor counts as a line.
 */
export function parseDiff(patch: string): ParsedDiff {
  const lines: DiffLine[] = [];
  let added = 0;
  let removed = 0;
  let oldNo = 0;
  let newNo = 0;
  let started = false;
  // A trailing newline ends the last line; it is not an extra empty one.
  const raws = patch.split("\n");
  if (raws.at(-1) === "") raws.pop();
  for (const raw of raws) {
    if (raw.startsWith("@@")) {
      started = true;
      const match = /@@ -(\d+)(?:,\d+)? \+(\d+)/.exec(raw);
      oldNo = match ? Number(match[1]) : 0;
      newNo = match ? Number(match[2]) : 0;
      lines.push({ kind: "hunk", text: raw });
    } else if (!started) {
    } else if (raw.startsWith("\\")) {
      lines.push({ kind: "note", text: raw.slice(1).trim() });
    } else if (raw.startsWith("+")) {
      added++;
      lines.push({ kind: "add", newNo: newNo++, text: raw.slice(1) });
    } else if (raw.startsWith("-")) {
      removed++;
      lines.push({ kind: "del", oldNo: oldNo++, text: raw.slice(1) });
    } else {
      lines.push({
        kind: "ctx",
        oldNo: oldNo++,
        newNo: newNo++,
        text: raw.startsWith(" ") ? raw.slice(1) : raw,
      });
    }
  }
  return { lines, added, removed };
}
