import type { EventFrame } from "../../shared/protocol";

export const MAX_BUFFER_BYTES = 5 * 1024 * 1024;

interface Entry {
  frames: Array<{ frame: EventFrame; bytes: number }>;
  bytes: number;
  /** The turn that has begun and not yet ended, if any. */
  active: { sessionId: string; turnId: string } | null;
}

export interface Attached {
  frames: EventFrame[];
  /** The id of the running turn when it belongs to this session, else `null`. */
  activeTurnId: string | null;
}

/**
 * The frames of each workspace's current or most recent turn, so a reloaded window can catch up.
 * It is a cache of frames already sent: a new turn replaces it, closing the workspace or a crash
 * empties it, and the oldest frames go first past about 5 MiB.
 */
export class LiveBuffers {
  private readonly entries = new Map<string, Entry>();

  push(frame: EventFrame): void {
    let entry = this.entries.get(frame.workspace);
    const type = frame.event.type;
    if (type === "turn.created" || !entry) {
      entry = { frames: [], bytes: 0, active: null };
      this.entries.set(frame.workspace, entry);
    }
    if (type === "turn.created" && frame.turn_id) {
      entry.active = { sessionId: frame.session_id, turnId: frame.turn_id };
    } else if (type === "turn.done") {
      entry.active = null;
    }
    const bytes = JSON.stringify(frame).length;
    entry.frames.push({ frame, bytes });
    entry.bytes += bytes;
    // The newest frame always stays, even when it alone is over the limit.
    while (entry.bytes > MAX_BUFFER_BYTES && entry.frames.length > 1) {
      entry.bytes -= entry.frames.shift()?.bytes ?? 0;
    }
  }

  /** The buffered frames of one session and whether its turn is the running one, in one step. */
  attach(workspace: string, sessionId: string): Attached {
    const entry = this.entries.get(workspace);
    if (!entry) return { frames: [], activeTurnId: null };
    return {
      frames: entry.frames.filter((f) => f.frame.session_id === sessionId).map((f) => f.frame),
      activeTurnId: entry.active?.sessionId === sessionId ? entry.active.turnId : null,
    };
  }

  /** Whether any workspace has a turn that has begun and not ended. */
  get running(): boolean {
    return [...this.entries.values()].some((entry) => entry.active !== null);
  }

  /** One workspace's buffer, or every buffer. */
  clear(workspace?: string): void {
    if (workspace === undefined) this.entries.clear();
    else this.entries.delete(workspace);
  }
}
