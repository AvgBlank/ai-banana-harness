// The sidecar's process owner: spawn, handshake, framing, crash and restart policy, shutdown.
// It never restarts a deterministic failure and never re-sends or retries a request.
import type { EventEmitter } from "node:events";
import type { Readable, Writable } from "node:stream";
import {
  type EventFrame,
  type MethodName,
  type Params,
  PROTOCOL_VERSION,
  type Result,
  type SidecarFrame,
} from "../../shared/protocol";
import { LineWriter, RpcClient, RpcError } from "./client";
import { redact } from "./env";
import { LineSplitter, parseFrame } from "./framing";

export const HANDSHAKE_TIMEOUT_MS = 10_000;
export const RESTART_DELAYS_MS = [500, 1_000, 2_000];
export const RESTART_WINDOW_MS = 60_000;
export const STABLE_AFTER_MS = 60_000;
export const MAX_CONSECUTIVE_INVALID = 20;
export const SHUTDOWN_REQUEST_MS = 4_000;
export const EXIT_WAIT_MS = 5_000;
export const TERM_WAIT_MS = 2_000;
const STDERR_TAIL_BYTES = 4 * 1024;

/** The part of a child process the supervisor uses; a scripted fake implements it in tests. */
export interface ChildLike extends EventEmitter {
  stdin: Writable;
  stdout: Readable;
  stderr: Readable;
  kill(signal?: NodeJS.Signals): boolean;
}

export type Spawn = (
  bin: string,
  args: string[],
  options: { env: Record<string, string> },
) => ChildLike;

export type FailureReason =
  | "no_ready"
  | "incompatible_protocol"
  | "fatal"
  | "exited_early"
  | "restart_budget";

export type SidecarState =
  | { state: "starting" }
  | { state: "ready"; reopened: string[] }
  | { state: "needs_key" }
  | { state: "crashed"; attempt: number; restartInMs: number }
  | { state: "failed"; reason: FailureReason; message: string }
  | { state: "stopping" }
  | { state: "stopped" };

export interface SupervisorOptions {
  bin: string;
  spawn: Spawn;
  /** Called for every spawn: the sidecar's whole environment and the key inside it, if any. */
  environment: () => { variables: Record<string, string>; apiKey?: string };
  /** Opens one workspace again after a restart; the supervisor calls it one at a time. */
  reopenWorkspace?: (workspace: string) => Promise<void>;
  /** Count-and-kind diagnostics only: never frame content, params or results. */
  log?: (message: string) => void;
}

interface Run {
  child: ChildLike;
  client: RpcClient;
  splitter: LineSplitter;
  apiKey: string | undefined;
  ready: boolean;
  /** Set once this run's outcome is decided, so its later events change nothing. */
  settled: boolean;
  exited: boolean;
  invalidInARow: number;
  stderr: Buffer;
  handshakeTimer?: ReturnType<typeof setTimeout>;
  stableTimer?: ReturnType<typeof setTimeout>;
}

type Listener<T> = (value: T) => void;

export class SidecarSupervisor {
  private current: SidecarState = { state: "stopped" };
  private run: Run | undefined;
  private restarts: number[] = [];
  private restartTimer?: ReturnType<typeof setTimeout>;
  private afterRestart = false;
  private openWorkspaces: string[] = [];
  private dropped = 0;
  private stopping: Promise<void> | undefined;
  private readonly stateListeners = new Set<Listener<SidecarState>>();
  private readonly eventListeners = new Set<Listener<EventFrame>>();

  constructor(private readonly options: SupervisorOptions) {}

  get state(): SidecarState {
    return this.current;
  }

  /** Lines dropped as oversized, non-UTF-8, non-JSON or not a protocol frame. */
  get droppedLines(): number {
    return this.dropped;
  }

  onState(listener: Listener<SidecarState>): () => void {
    this.stateListeners.add(listener);
    return () => this.stateListeners.delete(listener);
  }

  onEvent(listener: Listener<EventFrame>): () => void {
    this.eventListeners.add(listener);
    return () => this.eventListeners.delete(listener);
  }

  /** The workspaces to open again after a crash, in order. */
  setOpenWorkspaces(workspaces: string[]): void {
    this.openWorkspaces = [...workspaces];
  }

  start(): void {
    if (this.run || this.stopping) return;
    this.launch();
  }

  /** The user's Restart after `failed` (or a new key): a fresh budget and a fresh process. */
  restart(): void {
    if (this.stopping) return;
    clearTimeout(this.restartTimer);
    this.restarts = [];
    this.afterRestart = false;
    const run = this.run;
    if (run) this.discard(run);
    this.launch();
  }

  request<M extends MethodName>(method: M, params: Params<M>): Promise<Result<M>> {
    const run = this.run;
    if (!run?.ready || run.settled) {
      return Promise.reject(new RpcError("sidecar_unavailable", "the sidecar is not running"));
    }
    return run.client.request(method, params);
  }

  /**
   * Asks the sidecar to shut down, closes stdin, then escalates: `shutdown` (4 s), a wait for
   * exit (5 s), SIGTERM, SIGKILL after 2 s. `force` skips straight to the kill path.
   */
  shutdown(force = false): Promise<void> {
    if (this.stopping && !force) return this.stopping;
    this.stopping = this.stop(force);
    return this.stopping;
  }

  private async stop(force: boolean): Promise<void> {
    clearTimeout(this.restartTimer);
    const run = this.run;
    this.setState({ state: "stopping" });
    if (run && !run.exited) {
      if (!force) {
        if (run.ready) {
          await atMost(
            SHUTDOWN_REQUEST_MS,
            run.client.request("shutdown", {}).catch(() => undefined),
          );
        }
        run.child.stdin.end();
        if (await this.exitedWithin(run, EXIT_WAIT_MS)) return this.finishStop(run);
      }
      run.child.kill("SIGTERM");
      if (!force && (await this.exitedWithin(run, TERM_WAIT_MS))) return this.finishStop(run);
      run.child.kill("SIGKILL");
      await this.exitedWithin(run, TERM_WAIT_MS);
    }
    this.finishStop(run);
  }

  private finishStop(run: Run | undefined): void {
    if (run) this.discard(run, false);
    this.run = undefined;
    this.setState({ state: "stopped" });
  }

  private exitedWithin(run: Run, ms: number): Promise<boolean> {
    if (run.exited) return Promise.resolve(true);
    return new Promise((resolve) => {
      const timer = setTimeout(() => resolve(false), ms);
      run.child.once("exit", () => {
        clearTimeout(timer);
        resolve(true);
      });
    });
  }

  // ---- launching and the handshake ----

  private launch(): void {
    const { variables, apiKey } = this.options.environment();
    const child = this.options.spawn(this.options.bin, ["serve", "--stdio"], { env: variables });
    const writer = new LineWriter(child.stdin);
    const run: Run = {
      child,
      client: new RpcClient(
        (line) => writer.write(line),
        (text) => redact(text, apiKey),
      ),
      splitter: new LineSplitter(),
      apiKey,
      ready: false,
      settled: false,
      exited: false,
      invalidInARow: 0,
      stderr: Buffer.alloc(0),
    };
    this.run = run;
    this.setState({ state: "starting" });

    run.handshakeTimer = setTimeout(() => {
      if (run.ready || run.settled) return;
      this.discard(run);
      this.fail("no_ready", "the sidecar did not say ready within 10 s");
    }, HANDSHAKE_TIMEOUT_MS);

    child.stdout.on("data", (chunk: Buffer) => this.onStdout(run, chunk));
    child.stderr.on("data", (chunk: Buffer) => {
      run.stderr = Buffer.concat([run.stderr, chunk]).subarray(-STDERR_TAIL_BYTES);
    });
    // stdin breaking (EPIPE) means the process is going away; the exit event decides what it was.
    child.stdin.on("error", () => undefined);
    child.on("error", () => this.onExit(run));
    child.on("exit", () => this.onExit(run));
  }

  private onStdout(run: Run, chunk: Buffer): void {
    for (const piece of run.splitter.push(chunk)) {
      if (run.settled) return;
      const frame = piece.kind === "line" ? parseFrame(piece.text) : null;
      if (!frame) {
        this.invalid(run, piece.kind === "dropped" ? piece.reason : "not_a_frame");
        continue;
      }
      run.invalidInARow = 0;
      this.onFrame(run, frame);
    }
  }

  private invalid(run: Run, kind: string): void {
    this.dropped++;
    run.invalidInARow++;
    this.options.log?.(`dropped a sidecar line (${kind}); ${this.dropped} so far`);
    if (!run.ready || run.invalidInARow >= MAX_CONSECUTIVE_INVALID) {
      this.discard(run);
      this.crashed(run);
    }
  }

  private onFrame(run: Run, frame: SidecarFrame): void {
    if (!run.ready) {
      if (frame.kind === "fatal") {
        this.discard(run);
        this.fail("fatal", redact(frame.error.message, run.apiKey));
      } else if (frame.kind === "ready") {
        this.onReady(run, frame);
      } else {
        this.invalid(run, "frame_before_ready");
      }
      return;
    }
    if (frame.kind === "response") run.client.handleResponse(frame);
    else if (frame.kind === "event") for (const listener of this.eventListeners) listener(frame);
    else this.invalid(run, `unexpected_${frame.kind}`);
  }

  private onReady(run: Run, frame: { protocol: number; api_key_present: boolean }): void {
    clearTimeout(run.handshakeTimer);
    if (frame.protocol !== PROTOCOL_VERSION) {
      this.discard(run);
      this.fail(
        "incompatible_protocol",
        `the sidecar speaks protocol ${frame.protocol}, this app speaks ${PROTOCOL_VERSION}`,
      );
      return;
    }
    run.ready = true;
    // A run that stays up this long has earned a fresh restart budget.
    run.stableTimer = setTimeout(() => {
      this.restarts = [];
    }, STABLE_AFTER_MS);
    if (!frame.api_key_present) {
      this.setState({ state: "needs_key" });
      return;
    }
    void this.reopenThenReady(run);
  }

  private async reopenThenReady(run: Run): Promise<void> {
    const reopened: string[] = [];
    if (this.afterRestart && this.options.reopenWorkspace) {
      // One at a time: each open is a request the sidecar answers before the next is sent.
      for (const workspace of [...this.openWorkspaces]) {
        if (run.settled) return;
        try {
          await this.options.reopenWorkspace(workspace);
          reopened.push(workspace);
        } catch {
          this.options.log?.("a workspace could not be reopened after the restart");
        }
      }
    }
    if (run.settled) return;
    this.afterRestart = false;
    this.setState({ state: "ready", reopened });
  }

  // ---- failure, crash and restart ----

  private onExit(run: Run): void {
    run.exited = true;
    if (run.settled || this.stopping) {
      this.finishRun(run);
      return;
    }
    if (!run.ready) {
      this.finishRun(run);
      const tail = redact(run.stderr.toString("utf8"), run.apiKey);
      this.fail("exited_early", tail);
      return;
    }
    this.crashed(run);
  }

  /** Stops a run's timers and rejects its pending requests; with `kill` also ends the process. */
  private discard(run: Run, kill = true): void {
    run.settled = true;
    this.finishRun(run);
    if (kill && !run.exited) run.child.kill("SIGKILL");
  }

  private finishRun(run: Run): void {
    clearTimeout(run.handshakeTimer);
    clearTimeout(run.stableTimer);
    run.client.rejectAll("sidecar_exited", "the sidecar stopped");
  }

  private fail(reason: FailureReason, message: string): void {
    this.setState({ state: "failed", reason, message });
  }

  private crashed(run: Run): void {
    run.settled = true;
    this.finishRun(run);
    if (!run.exited) run.child.kill("SIGKILL");
    const now = Date.now();
    this.restarts = this.restarts.filter((at) => now - at < RESTART_WINDOW_MS);
    if (this.restarts.length >= RESTART_DELAYS_MS.length) {
      this.fail("restart_budget", "the sidecar kept stopping; restart it to try again");
      return;
    }
    const restartInMs = RESTART_DELAYS_MS[this.restarts.length] ?? 2_000;
    this.restarts.push(now);
    this.afterRestart = true;
    this.setState({ state: "crashed", attempt: this.restarts.length, restartInMs });
    this.restartTimer = setTimeout(() => this.launch(), restartInMs);
  }

  private setState(next: SidecarState): void {
    this.current = next;
    for (const listener of this.stateListeners) listener(next);
  }
}

/** Resolves when `work` does or after `ms`, whichever is first; the timer never outlives it. */
async function atMost(ms: number, work: Promise<unknown>): Promise<void> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  await Promise.race([
    work,
    new Promise<void>((resolve) => {
      timer = setTimeout(resolve, ms);
    }),
  ]);
  clearTimeout(timer);
}
