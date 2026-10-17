import { type SidecarFrame, sidecarFrame } from "../../shared/protocol";

/** The longest line accepted from the sidecar; a longer one is discarded to its newline. */
export const MAX_LINE_BYTES = 8 * 1024 * 1024;

export type LineEvent =
  | { kind: "line"; text: string }
  | { kind: "dropped"; reason: "oversized" | "invalid_utf8" };

const NEWLINE = 0x0a;

/** Splits a byte stream into UTF-8 lines. Oversized and non-UTF-8 lines come out as `dropped`. */
export class LineSplitter {
  private chunks: Buffer[] = [];
  private length = 0;
  private discarding = false;

  push(chunk: Buffer): LineEvent[] {
    const events: LineEvent[] = [];
    let rest = chunk;
    while (rest.length > 0) {
      const newline = rest.indexOf(NEWLINE);
      const segment = newline < 0 ? rest : rest.subarray(0, newline);
      if (!this.discarding) {
        if (this.length + segment.length > MAX_LINE_BYTES) {
          this.discarding = true;
          this.chunks = [];
          this.length = 0;
        } else {
          this.chunks.push(segment);
          this.length += segment.length;
        }
      }
      if (newline < 0) break;
      events.push(this.finish());
      rest = rest.subarray(newline + 1);
    }
    return events;
  }

  private finish(): LineEvent {
    const wasDiscarding = this.discarding;
    const bytes = Buffer.concat(this.chunks);
    this.chunks = [];
    this.length = 0;
    this.discarding = false;
    if (wasDiscarding) return { kind: "dropped", reason: "oversized" };
    try {
      return { kind: "line", text: new TextDecoder("utf-8", { fatal: true }).decode(bytes) };
    } catch {
      return { kind: "dropped", reason: "invalid_utf8" };
    }
  }
}

/** One line to a frame, or `null` when it is not JSON or not a frame the protocol defines. */
export function parseFrame(line: string): SidecarFrame | null {
  let value: unknown;
  try {
    value = JSON.parse(line);
  } catch {
    return null;
  }
  const parsed = sidecarFrame.safeParse(value);
  return parsed.success ? parsed.data : null;
}
