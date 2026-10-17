import { spawn } from "node:child_process";

export const PROBE_TIMEOUT_MS = 5_000;
/** More output than this is cut off; an environment is a few kilobytes. */
const MAX_OUTPUT_BYTES = 256 * 1024;
export const PROBE_BEGIN = "__BH_PROBE_BEGIN__";
export const PROBE_END = "__BH_PROBE_END__";

/** `printenv` and `printf` exist in every shell (zsh, bash, fish); the sentinels fence off rc-file noise. */
const SCRIPT = `printf '%s\\n' ${PROBE_BEGIN}; printenv; printf '%s\\n' ${PROBE_END}`;

/** The variables the app wants from the user's own shell. */
const WANTED = /^(PATH|AI_API_KEY|http_proxy|https_proxy|all_proxy|no_proxy)$/i;

export type ProbeResult = Record<string, string>;

/** The wanted variables between the sentinels, or `null` when there is no complete block. */
export function parseProbeOutput(output: string): ProbeResult | null {
  const start = output.lastIndexOf(PROBE_BEGIN);
  const end = output.lastIndexOf(PROBE_END);
  if (start < 0 || end < start) return null;
  const found: ProbeResult = {};
  for (const line of output.slice(start + PROBE_BEGIN.length, end).split("\n")) {
    const equals = line.indexOf("=");
    if (equals <= 0) continue;
    const name = line.slice(0, equals);
    const value = line.slice(equals + 1).trim();
    if (WANTED.test(name) && value) found[name] = value;
  }
  return found;
}

/**
 * Runs the user's login shell (`$SHELL -ilc`) once, with no stdin, a hard timeout and an output
 * cap. A shell that hangs is killed with its whole process group; failure is `null`, never an
 * error that blocks the app.
 */
export function probeLoginShell(
  shell: string | undefined,
  timeoutMs: number = PROBE_TIMEOUT_MS,
): Promise<ProbeResult | null> {
  if (!shell) return Promise.resolve(null);
  return new Promise((resolve) => {
    let output = "";
    let settled = false;
    const finish = (result: ProbeResult | null) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(result);
    };
    let child: ReturnType<typeof spawn>;
    try {
      // Its own process group, so the kill below reaches anything the rc files started.
      child = spawn(shell, ["-ilc", SCRIPT], {
        stdio: ["ignore", "pipe", "ignore"],
        detached: true,
      });
    } catch {
      resolve(null);
      return;
    }
    const kill = () => {
      try {
        if (child.pid) process.kill(-child.pid, "SIGKILL");
      } catch {
        // Already gone.
      }
    };
    const timer = setTimeout(() => {
      kill();
      finish(null);
    }, timeoutMs);
    child.stdout?.on("data", (chunk: Buffer) => {
      if (output.length < MAX_OUTPUT_BYTES) output += chunk.toString("utf8");
      else kill();
    });
    child.on("error", () => finish(null));
    child.on("close", () => finish(parseProbeOutput(output)));
  });
}
