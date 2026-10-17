import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import type { ChildLike } from "../../../src/main/sidecar/supervisor";

/** A scripted stand-in for the sidecar process: tests feed its stdout and read what it was sent. */
export class FakeChild extends EventEmitter implements ChildLike {
  stdin = new PassThrough();
  stdout = new PassThrough();
  stderr = new PassThrough();
  signals: string[] = [];
  /** Every line written to the child's stdin, parsed. */
  received: Array<{ id: string; method: string; params: unknown }> = [];
  private buffer = "";
  /** Set to reply to requests as they arrive. */
  respond: ((request: { id: string; method: string; params: unknown }) => unknown) | undefined;
  ended = false;

  constructor() {
    super();
    this.stdin.on("data", (chunk: Buffer) => {
      this.buffer += chunk.toString("utf8");
      let newline = this.buffer.indexOf("\n");
      while (newline >= 0) {
        const line = this.buffer.slice(0, newline);
        this.buffer = this.buffer.slice(newline + 1);
        const request = JSON.parse(line);
        this.received.push(request);
        const reply = this.respond?.(request);
        if (reply !== undefined)
          this.frame({ kind: "response", id: request.id, ok: true, result: reply });
        newline = this.buffer.indexOf("\n");
      }
    });
    this.stdin.on("end", () => {
      this.ended = true;
    });
  }

  kill(signal: NodeJS.Signals = "SIGTERM"): boolean {
    this.signals.push(signal);
    return true;
  }

  frame(value: unknown): void {
    this.stdout.write(`${JSON.stringify(value)}\n`);
  }

  raw(data: string | Buffer): void {
    this.stdout.write(data);
  }

  ready(overrides: Record<string, unknown> = {}): void {
    this.frame({
      kind: "ready",
      protocol: 1,
      app: "app",
      version: "0.1.0",
      api_key_present: true,
      ...overrides,
    });
  }

  exit(code = 0): void {
    this.emit("exit", code, null);
  }
}
