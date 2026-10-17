import type { Writable } from "node:stream";
import { type MethodName, methods, type Params, type Result } from "../../shared/protocol";

export const DEFAULT_TIMEOUT_MS = 30_000;
export const LONG_TIMEOUT_MS = 60_000;
const LONG_METHODS = new Set<string>(["workspace.open", "session.resume", "session.undo"]);

export function timeoutFor(method: string): number {
  return LONG_METHODS.has(method) || method.startsWith("changes.")
    ? LONG_TIMEOUT_MS
    : DEFAULT_TIMEOUT_MS;
}

/** A failure a caller can branch on: the sidecar's own error code, or one of main's. */
export class RpcError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "RpcError";
  }
}

/** Writes lines to a stream in order, waiting for `drain` when the stream pushes back. */
export class LineWriter {
  private tail: Promise<void> = Promise.resolve();

  constructor(private readonly stream: Writable) {}

  write(line: string): Promise<void> {
    const next = this.tail.then(
      () =>
        new Promise<void>((resolve, reject) => {
          const flushed = this.stream.write(`${line}\n`, (error) => {
            if (error) reject(error);
          });
          if (flushed) resolve();
          else this.stream.once("drain", resolve);
        }),
    );
    // A failed write must not poison the lines queued behind it; each caller sees its own result.
    this.tail = next.catch(() => undefined);
    return next;
  }
}

interface Pending {
  method: MethodName;
  resolve: (value: unknown) => void;
  reject: (error: RpcError) => void;
  timer: ReturnType<typeof setTimeout>;
}

/** Request/response correlation. Ids are generated here (`m<n>`), never taken from a caller. */
export class RpcClient {
  private nextId = 0;
  private readonly pending = new Map<string, Pending>();

  constructor(
    private readonly send: (line: string) => Promise<void>,
    private readonly redact: (text: string) => string,
  ) {}

  request<M extends MethodName>(method: M, params: Params<M>): Promise<Result<M>> {
    const parsed = methods[method].params.safeParse(params);
    if (!parsed.success) {
      return Promise.reject(new RpcError("invalid_params", `invalid params for ${method}`));
    }
    const id = `m${++this.nextId}`;
    return new Promise<Result<M>>((resolve, reject) => {
      const timer = setTimeout(() => {
        // A response that arrives later finds no entry and is ignored.
        this.pending.delete(id);
        reject(new RpcError("timeout", `${method} timed out`));
      }, timeoutFor(method));
      this.pending.set(id, { method, resolve: resolve as (v: unknown) => void, reject, timer });
      const line = JSON.stringify({ kind: "request", id, method, params: parsed.data });
      this.send(line).catch(() =>
        this.fail(id, new RpcError("sidecar_exited", "the sidecar is gone")),
      );
    });
  }

  /** A response frame from the sidecar. Unknown or already settled ids are dropped. */
  handleResponse(frame: {
    id: string | null;
    ok: boolean;
    result?: unknown;
    error?: { code: string; message: string };
  }): void {
    if (frame.id === null) return;
    const entry = this.pending.get(frame.id);
    if (!entry) return;
    this.pending.delete(frame.id);
    clearTimeout(entry.timer);
    if (!frame.ok) {
      const error = frame.error ?? { code: "internal", message: "error" };
      entry.reject(new RpcError(error.code, this.redact(error.message)));
      return;
    }
    const result = methods[entry.method].result.safeParse(frame.result);
    if (result.success) entry.resolve(result.data);
    else
      entry.reject(new RpcError("invalid_result", `${entry.method} returned an unexpected result`));
  }

  /** The sidecar is gone: every request still waiting fails. */
  rejectAll(code: string, message: string): void {
    for (const id of [...this.pending.keys()]) this.fail(id, new RpcError(code, message));
  }

  get pendingCount(): number {
    return this.pending.size;
  }

  private fail(id: string, error: RpcError): void {
    const entry = this.pending.get(id);
    if (!entry) return;
    this.pending.delete(id);
    clearTimeout(entry.timer);
    entry.reject(error);
  }
}
