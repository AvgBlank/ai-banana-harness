// Stands in for the sidecar in one test: runs the real binary and, after every event frame, adds a
// line the schema must refuse. Usage: node noisy-sidecar.mjs <real binary> [args...]
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

const [real, ...args] = process.argv.slice(2);
const child = spawn(real, args, { env: process.env, stdio: ["pipe", "pipe", "inherit"] });
process.stdin.pipe(child.stdin);
child.stdin.on("error", () => undefined);
process.stdin.on("end", () => child.stdin.end());

const junk = JSON.stringify({
  kind: "event",
  workspace: "/w",
  session_id: "s",
  turn_id: null,
  event: { id: "x", type: "not.a.kind" },
});
createInterface({ input: child.stdout }).on("line", (line) => {
  process.stdout.write(`${line}\n`);
  if (line.startsWith('{"event"')) process.stdout.write(`${junk}\n`);
});
child.on("exit", (code) => process.exit(code ?? 0));
process.on("SIGTERM", () => child.kill("SIGTERM"));
